use std::ops::{DerefMut, Div};

use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ard_pal::prelude::*;
use ard_render_base::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, consts::*, types::*};
use ordered_float::NotNan;

const PREFILTERED_ENV_MAP_DIM: u32 = 256;
const DIFFUSE_IRRADIANCE_MAP_DIM: u32 = 64;
const DIFFUSE_IRRADIANCE_SAMPLE_DIM: u64 = DI_REDUCE_BLOCK_SIZE as u64;

pub const DI_MAP_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

pub struct ProceduralSkyBox {
    /// Pipeline for rendering the sky box.
    sky_box_pipeline: GraphicsPipeline,
    /// Pipeline for rendering the sky box during the color pass.
    color_pass_skybox_pipeline: GraphicsPipeline,
    /// Pipeline for gathering diffuse irradiance spherical harmonic coefficients.
    di_gather_pipeline: ComputePipeline,
    di_gather_set: DescriptorSet,
    /// Pipeline for summing all coefficients together.
    di_par_reduce_pipeline: ComputePipeline,
    di_par_reduce_set: DescriptorSet,
    // Pipeline for rendering the diffuse irradiance map.
    di_render_pipeline: GraphicsPipeline,
    di_render_set: DescriptorSet,
    // Pipeline for prefiltering the environment map.
    prefilter_pipeline: GraphicsPipeline,
    prefilter_sets: Vec<DescriptorSet>,
    // Camera UBO for rendering the diffuse irradiance map.
    di_render_camera: CameraUbo,
    /// Samples for computing diffuse irradiance.
    _diffuse_irradiance_samples: Buffer,
    // Prefiltering matrices. Output from irradiance gather pipeline.
    _prefiltering_matrices: Buffer,
    // Environment prefiltering info.
    _env_prefilter_info: Buffer,
    /// BRDF lookup texture.
    brdf_lut: Texture,
    // Skybox cube map with mip maps used for prefiltering.
    sky_box: CubeMap,
    // Prefiltered environment map.
    prefiltered_map: CubeMap,
    // Diffuse irradiance map.
    di_map: CubeMap,
}

impl ProceduralSkyBox {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        // Buffer for irradiance samples
        let diffuse_irradiance_samples = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: Self::diffuse_irradiance_buffer_size(),
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("diffuse_irradiance_samples".into()),
            },
        )
        .unwrap();

        // Buffer for prefiltering matrices
        let prefiltering_matrices = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<GpuPrefilteringMatrices>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("prefiltering_matrices".into()),
            },
        )
        .unwrap();

        // Sky box.
        let sky_box = CubeMap::new(
            ctx.clone(),
            CubeMapCreateInfo {
                format: Format::Rgba16SFloat,
                size: PREFILTERED_ENV_MAP_DIM,
                array_elements: 1,
                mip_levels: (PREFILTERED_ENV_MAP_DIM as f32).log2() as usize + 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_DST
                    | TextureUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sky_box".into()),
            },
        )
        .unwrap();

        // Prefiltered environment map.
        let prefiltered_map = CubeMap::new(
            ctx.clone(),
            CubeMapCreateInfo {
                format: Format::Rgba16SFloat,
                size: PREFILTERED_ENV_MAP_DIM,
                array_elements: 1,
                mip_levels: (PREFILTERED_ENV_MAP_DIM as f32).log2() as usize + 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("prefiltered_env_map".into()),
            },
        )
        .unwrap();

        // Diffuse irradiance map.
        let di_map = CubeMap::new(
            ctx.clone(),
            CubeMapCreateInfo {
                format: Format::Rgba16SFloat,
                size: DIFFUSE_IRRADIANCE_MAP_DIM,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("di_map".into()),
            },
        )
        .unwrap();

        let brdf_lut = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rg16SFloat,
                ty: TextureType::Type2D,
                width: 512,
                height: 512,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("brdf_lut".into()),
            },
        )
        .unwrap();

        let brdf_lut_staging = Buffer::new_staging(
            ctx.clone(),
            QueueType::Main,
            Some(String::from("brdf_lut_staging")),
            include_bytes!("../data/brdf_lut.bin"),
        )
        .unwrap();

        let mut cb = ctx.main().command_buffer();
        cb.copy_buffer_to_texture(
            &brdf_lut,
            &brdf_lut_staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (512, 512, 1),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        ctx.main().submit(Some("brdf_lut_write"), cb);

        // Camera for rendering the diffuse irradiance map
        let mut di_render_camera = CameraUbo::new(ctx, false, layouts);

        const DIRS: [(Vec3, Vec3); 6] = [
            (Vec3::X, Vec3::Y),
            (Vec3::NEG_X, Vec3::Y),
            (Vec3::Y, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::Z),
            (Vec3::Z, Vec3::Y),
            (Vec3::NEG_Z, Vec3::Y),
        ];

        for (i, (forward, up)) in DIRS.into_iter().enumerate() {
            let view = Mat4::look_at_lh(Vec3::ZERO, forward, up);
            let proj =
                Mat4::perspective_infinite_reverse_lh(std::f32::consts::FRAC_PI_2, 1.0, 0.25);

            let vp = proj * view;
            let mut frustum = GpuFrustum::from(vp);
            frustum.planes[4] = Vec4::ZERO;

            di_render_camera.update_raw(
                Frame::from(0),
                &GpuCamera {
                    view,
                    projection: proj,
                    vp,
                    last_vp: vp,
                    view_inv: view.inverse(),
                    projection_inv: proj.inverse(),
                    vp_inv: vp.inverse(),
                    frustum,
                    position: Vec4::from((Vec3::ZERO, 1.0)),
                    last_position: Vec4::from((Vec3::ZERO, 1.0)),
                    forward: Vec4::from((forward, 0.0)),
                    aspect_ratio: 1.0,
                    near_clip: 1.0,
                    far_clip: 1.0,
                    cluster_scale_bias: Vec2::ONE,
                },
                i,
            );
        }

        // Graphics pipeline
        let vs = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./proc_skybox.vert.spv")),
                debug_name: Some("proc_skybox_vertex_shader".into()),
            },
        )
        .unwrap();

        let fs = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./proc_skybox.frag.spv")),
                debug_name: Some("proc_skybox_fragment_shader".into()),
            },
        )
        .unwrap();

        let sky_box_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: vs.clone(),
                    fragment: Some(fs),
                },
                layouts: vec![layouts.camera.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: false,
                    depth_compare: CompareOp::Equal,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: false,
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        ..Default::default()
                    }],
                },
                push_constants_size: Some(
                    std::mem::size_of::<GpuSkyBoxRenderPushConstants>() as u32
                ),
                debug_name: Some(String::from("sky_box_pipeline")),
            },
        )
        .unwrap();

        let fs = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "./proc_skybox.color_pass.frag.spv"
                )),
                debug_name: Some("color_pass_proc_skybox_fragment_shader".into()),
            },
        )
        .unwrap();

        let color_pass_skybox_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./proc_skybox.color_pass.vert.spv"
                            )),
                            debug_name: Some("color_pass_proc_skybox_vertex_shader".into()),
                        },
                    )
                    .unwrap(),
                    fragment: Some(fs),
                },
                layouts: vec![layouts.camera.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: false,
                    depth_compare: CompareOp::Equal,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState {
                    attachments: vec![
                        // Color
                        ColorBlendAttachment {
                            blend: false,
                            write_mask: ColorComponents::R
                                | ColorComponents::G
                                | ColorComponents::B,
                            ..Default::default()
                        },
                        // Thin G
                        ColorBlendAttachment {
                            blend: false,
                            write_mask: ColorComponents::all(),
                            ..Default::default()
                        },
                        // Vel
                        ColorBlendAttachment {
                            blend: false,
                            write_mask: ColorComponents::R | ColorComponents::G,
                            ..Default::default()
                        },
                        // Norm
                        ColorBlendAttachment {
                            blend: false,
                            write_mask: ColorComponents::R
                                | ColorComponents::G
                                | ColorComponents::B,
                            ..Default::default()
                        },
                    ],
                },
                push_constants_size: Some(
                    std::mem::size_of::<GpuSkyBoxRenderPushConstants>() as u32
                ),
                debug_name: Some(String::from("color_pass_sky_box_pipeline")),
            },
        )
        .unwrap();

        let mut di_render_set = DescriptorSet::new(
            ctx.clone(),
            DescriptorSetCreateInfo {
                layout: layouts.di_render.clone(),
                debug_name: Some("di_render_set".into()),
            },
        )
        .unwrap();

        di_render_set.update(&[DescriptorSetUpdate {
            binding: DI_RENDER_SET_DI_PREFILTERING_MATS_BINDING,
            array_element: 0,
            value: DescriptorValue::UniformBuffer {
                buffer: &prefiltering_matrices,
                array_element: 0,
            },
        }]);

        // Diffuse irradiance render pipeline
        let fs = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./di_render.frag.spv")),
                debug_name: Some("diffuse_irradiance_fragment_shader".into()),
            },
        )
        .unwrap();

        let di_render_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: vs.clone(),
                    fragment: Some(fs),
                },
                layouts: vec![layouts.camera.clone(), layouts.di_render.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: None,
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: false,
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        ..Default::default()
                    }],
                },
                push_constants_size: None,
                debug_name: Some(String::from("di_render_pipeline")),
            },
        )
        .unwrap();

        // Diffuse irradiance gather pipeline
        let di_gather_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.di_gather.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(env!("OUT_DIR"), "./di_gather.comp.spv")),
                        debug_name: Some("di_gather_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32, 1, 1),
                push_constants_size: Some(std::mem::size_of::<GpuDiGatherPushConstants>() as u32),
                debug_name: Some("di_gather_pipeline".into()),
            },
        )
        .unwrap();

        let mut di_gather_set = DescriptorSet::new(
            ctx.clone(),
            DescriptorSetCreateInfo {
                layout: layouts.di_gather.clone(),
                debug_name: Some("di_gather_set".into()),
            },
        )
        .unwrap();

        di_gather_set.update(&[DescriptorSetUpdate {
            binding: DI_GATHER_SET_DI_SAMPLES_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: &diffuse_irradiance_samples,
                array_element: 0,
            },
        }]);

        // Diffuse irradiance reduction pipeline
        let di_par_reduce_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.di_par_reduce.clone()],
                    module: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./di_par_reduce.comp.spv"
                            )),
                            debug_name: Some("di_par_reduce_shader".into()),
                        },
                    )
                    .unwrap(),
                    work_group_size: (DI_REDUCE_BLOCK_SIZE as u32, 1, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuDiParReducePushConstants>() as u32
                    ),
                    debug_name: Some("di_par_reduce_pipeline".into()),
                },
            )
            .unwrap();

        let mut di_par_reduce_set = DescriptorSet::new(
            ctx.clone(),
            DescriptorSetCreateInfo {
                layout: layouts.di_par_reduce.clone(),
                debug_name: Some("di_par_reduce_set".into()),
            },
        )
        .unwrap();

        di_par_reduce_set.update(&[
            DescriptorSetUpdate {
                binding: DI_PAR_REDUCE_SET_DI_SAMPLES_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &diffuse_irradiance_samples,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: DI_PAR_REDUCE_SET_DI_PREFILTERING_MATS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &prefiltering_matrices,
                    array_element: 0,
                },
            },
        ]);

        // Environment map prefiltering pipeline
        let fs = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./env_prefilter.frag.spv")),
                debug_name: Some("environment_map_prefiltering_fragment_shader".into()),
            },
        )
        .unwrap();

        let prefilter_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: vs.clone(),
                    fragment: Some(fs),
                },
                layouts: vec![layouts.camera.clone(), layouts.env_prefilter.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: None,
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: false,
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        ..Default::default()
                    }],
                },
                push_constants_size: Some(
                    std::mem::size_of::<GpuEnvPrefilterPushConstants>() as u32
                ),
                debug_name: Some(String::from("environment_map_prefiltering_pipeline")),
            },
        )
        .unwrap();

        let env_prefilter_info = Self::env_prefilter_info_buffer(ctx, sky_box.mip_count());

        let prefilter_sets = (0..sky_box.mip_count())
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.env_prefilter.clone(),
                        debug_name: Some("environment_map_prefiltering_set".into()),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: ENV_PREFILTER_SET_ENV_MAP_BINDING,
                        array_element: 0,
                        value: DescriptorValue::CubeMap {
                            array_element: 0,
                            cube_map: &sky_box,
                            sampler: DI_MAP_SAMPLER,
                            base_mip: 0,
                            mip_count: sky_box.mip_count(),
                        },
                    },
                    DescriptorSetUpdate {
                        binding: ENV_PREFILTER_SET_PREFILTER_INFO_BINDING,
                        array_element: 0,
                        value: DescriptorValue::UniformBuffer {
                            buffer: &env_prefilter_info,
                            array_element: i,
                        },
                    },
                ]);

                set
            })
            .collect();

        Self {
            sky_box_pipeline,
            color_pass_skybox_pipeline,
            _diffuse_irradiance_samples: diffuse_irradiance_samples,
            di_gather_pipeline,
            di_gather_set,
            di_par_reduce_pipeline,
            di_par_reduce_set,
            di_render_pipeline,
            di_render_camera,
            di_render_set,
            prefilter_pipeline,
            prefilter_sets,
            _prefiltering_matrices: prefiltering_matrices,
            _env_prefilter_info: env_prefilter_info,
            sky_box,
            prefiltered_map,
            di_map,
            brdf_lut,
        }
    }

    #[inline(always)]
    pub fn brdf_lut(&self) -> &Texture {
        &self.brdf_lut
    }

    #[inline(always)]
    pub fn di_map(&self) -> &CubeMap {
        &self.di_map
    }

    #[inline(always)]
    pub fn prefiltered_env_map(&self) -> &CubeMap {
        &self.prefiltered_map
    }

    pub fn gather_diffuse_irradiance<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        sun_direction: Vec3,
    ) {
        // Gather irradiance
        commands.compute_pass(&self.di_gather_pipeline, Some("di_gather"), |pass| {
            pass.bind_sets(0, vec![&self.di_gather_set]);
            let constants = [GpuDiGatherPushConstants {
                sample_dim: DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32,
                sun_direction: Vec4::from((sun_direction, 0.0)),
            }];
            pass.push_constants(bytemuck::cast_slice(&constants));
            ComputePassDispatch::Inline(1, DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32, 1)
        });

        // Reduce to prefiltering matrices
        commands.compute_pass(
            &self.di_par_reduce_pipeline,
            Some("di_par_reduce_first"),
            |pass| {
                pass.bind_sets(0, vec![&self.di_par_reduce_set]);
                let constants = [GpuDiParReducePushConstants {
                    block_size: DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32,
                    construct_prefiltering_matrices: 0,
                }];
                pass.push_constants(bytemuck::cast_slice(&constants));
                ComputePassDispatch::Inline(DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32, 1, 1)
            },
        );

        commands.compute_pass(
            &self.di_par_reduce_pipeline,
            Some("di_par_reduce_last"),
            |pass| {
                pass.bind_sets(0, vec![&self.di_par_reduce_set]);
                let constants = [GpuDiParReducePushConstants {
                    block_size: DIFFUSE_IRRADIANCE_SAMPLE_DIM as u32,
                    construct_prefiltering_matrices: 1,
                }];
                pass.push_constants(bytemuck::cast_slice(&constants));
                ComputePassDispatch::Inline(1, 1, 1)
            },
        );

        // Render the actual prefiltered map
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst: ColorAttachmentDestination::CubeMap {
                        cube_map: &self.di_map,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                depth_stencil_attachment: None,
                color_resolve_attachments: Vec::default(),
                depth_stencil_resolve_attachment: None,
            },
            Some("di_render"),
            |pass| {
                pass.bind_pipeline(self.di_render_pipeline.clone());
                pass.bind_sets(
                    0,
                    vec![
                        self.di_render_camera.get_set(Frame::from(0)),
                        &self.di_render_set,
                    ],
                );
                pass.draw(36, 1, 0, 0);
            },
        );
    }

    pub fn prefilter_environment_map<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        sun_direction: Vec3,
    ) {
        // Render the sky box
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst: ColorAttachmentDestination::CubeMap {
                        cube_map: &self.sky_box,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                depth_stencil_attachment: None,
                color_resolve_attachments: Vec::default(),
                depth_stencil_resolve_attachment: None,
            },
            Some("sky_box_render"),
            |pass| {
                pass.bind_pipeline(self.sky_box_pipeline.clone());
                pass.bind_sets(0, vec![self.di_render_camera.get_set(Frame::from(0))]);
                let constants = [GpuSkyBoxRenderPushConstants {
                    sun_direction: Vec4::from((sun_direction, 0.0)),
                }];
                pass.push_constants(bytemuck::cast_slice(&constants));
                pass.draw(36, 1, 0, 0);
            },
        );

        // Generate mip maps for the sky box
        const CUBE_FACES: [CubeFace; 6] = [
            CubeFace::Top,
            CubeFace::Bottom,
            CubeFace::North,
            CubeFace::East,
            CubeFace::South,
            CubeFace::West,
        ];

        let mut dim = self.sky_box.dim();
        for i in 1..self.sky_box.mip_count() {
            let new_dim = dim.div(2).max(1);
            for face in CUBE_FACES {
                commands.blit(
                    BlitSource::CubeMap {
                        cube_map: &self.sky_box,
                        face,
                    },
                    BlitDestination::CubeMap {
                        cube_map: &self.sky_box,
                        face,
                    },
                    Blit {
                        src_min: (0, 0, 0),
                        src_max: (dim, dim, 1),
                        src_mip: (i - 1) as usize,
                        src_array_element: 0,
                        dst_min: (0, 0, 0),
                        dst_max: (new_dim, new_dim, 1),
                        dst_mip: i as usize,
                        dst_array_element: 0,
                    },
                    Filter::Linear,
                );
            }
            dim = new_dim;
        }

        // Copy in mip 0 of the sky box to mip 0 of the environment map since they're identical
        for face in CUBE_FACES {
            commands.blit(
                BlitSource::CubeMap {
                    cube_map: &self.sky_box,
                    face,
                },
                BlitDestination::CubeMap {
                    cube_map: &self.prefiltered_map,
                    face,
                },
                Blit {
                    src_min: (0, 0, 0),
                    src_max: (PREFILTERED_ENV_MAP_DIM, PREFILTERED_ENV_MAP_DIM, 1),
                    src_mip: 0,
                    src_array_element: 0,
                    dst_min: (0, 0, 0),
                    dst_max: (PREFILTERED_ENV_MAP_DIM, PREFILTERED_ENV_MAP_DIM, 1),
                    dst_mip: 0,
                    dst_array_element: 0,
                },
                Filter::Linear,
            );
        }

        // Perform filtering for each mip level
        for mip_level in 1..self.prefiltered_map.mip_count() {
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![ColorAttachment {
                        dst: ColorAttachmentDestination::CubeMap {
                            cube_map: &self.prefiltered_map,
                            array_element: 0,
                            mip_level: mip_level,
                        },
                        load_op: LoadOp::DontCare,
                        store_op: StoreOp::Store,
                        samples: MultiSamples::Count1,
                    }],
                    depth_stencil_attachment: None,
                    color_resolve_attachments: Vec::default(),
                    depth_stencil_resolve_attachment: None,
                },
                Some("prefiltered_env_map"),
                |pass| {
                    pass.bind_pipeline(self.prefilter_pipeline.clone());
                    pass.bind_sets(
                        0,
                        vec![
                            self.di_render_camera.get_set(Frame::from(0)),
                            &self.prefilter_sets[mip_level],
                        ],
                    );
                    let constants = [GpuEnvPrefilterPushConstants {
                        roughness: mip_level as f32
                            / (self.prefiltered_map.mip_count() as f32 - 1.0),
                    }];
                    pass.push_constants(bytemuck::cast_slice(&constants));
                    pass.draw(36, 1, 0, 0);
                },
            );
        }
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut RenderPass<'a>,
        camera_set: &'a DescriptorSet,
        sun_direction: Vec3,
    ) {
        pass.bind_pipeline(self.color_pass_skybox_pipeline.clone());
        pass.bind_sets(0, vec![camera_set]);
        let constants = [GpuSkyBoxRenderPushConstants {
            sun_direction: Vec4::from((sun_direction, 0.0)),
        }];
        pass.push_constants(bytemuck::cast_slice(&constants));
        pass.draw(36, 1, 0, 0);
    }

    const fn diffuse_irradiance_buffer_size() -> u64 {
        // For each sample...
        (DIFFUSE_IRRADIANCE_SAMPLE_DIM * DIFFUSE_IRRADIANCE_SAMPLE_DIM) as u64
        // We have 9 values for our spherical harmonics and each value has three color channels,
        // making 27 values total, but we round up to 28 so we can use 7 Vec4s instead of floats.
        * 28 * std::mem::size_of::<f32>() as u64
    }

    fn env_prefilter_info_buffer(ctx: &Context, mip_count: usize) -> Buffer {
        let mut buff = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<GpuEnvPrefilterInfo>() as u64,
                array_elements: mip_count,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("env_prefilter_buffer".into()),
            },
        )
        .unwrap();

        fn radical_inverse_vdc(mut bits: u32) -> f32 {
            bits = (bits << 16) | (bits >> 16);
            bits = ((bits & 0x55555555) << 1) | ((bits & 0xAAAAAAAA) >> 1);
            bits = ((bits & 0x33333333) << 2) | ((bits & 0xCCCCCCCC) >> 2);
            bits = ((bits & 0x0F0F0F0F) << 4) | ((bits & 0xF0F0F0F0) >> 4);
            bits = ((bits & 0x00FF00FF) << 8) | ((bits & 0xFF00FF00) >> 8);
            (bits as f32) * 2.3283064365386963e-10 // 0x100000000
        }

        fn hammersley(i: u32, n: u32) -> Vec2 {
            Vec2::new((i as f32) / (n as f32), radical_inverse_vdc(i))
        }

        fn halfway_vector(roughness: f32, xi: Vec2) -> Vec3 {
            let a = roughness * roughness;

            let phi = 2.0 * std::f32::consts::PI * xi.x;
            let cos_theta = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).sqrt();
            let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

            // from spherical coordinates to cartesian coordinates
            Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta).normalize()
        }

        fn d_ggx(roughness: f32, ndoth: f32) -> f32 {
            let a = roughness * roughness;
            let a2 = a * a;
            let ndoth2 = ndoth * ndoth;
            let f = 1.0 + (ndoth2 * (a2 - 1.0));
            a2 / (f * f)
        }

        fn compute_mip_level(h: Vec3, roughness: f32, env_map_area: f32) -> f32 {
            const N: Vec3 = Vec3::Z;
            const V: Vec3 = Vec3::Z;

            // Vectors to evaluate pdf
            let fndoth = N.dot(h).clamp(0.0, 1.0);
            let fvdoth = V.dot(h).clamp(0.0, 1.0);

            // Probability Density Function
            let fpdf = d_ggx(roughness, fndoth) * fndoth / (4.0 * fvdoth);

            // Solid angle represented by this sample
            let fomegas = 1.0 / (ENV_PREFILTER_SAMPLE_COUNT as f32 * fpdf);

            // Solid angle covered by 1 pixel with 6 faces that are EnvMapSize X EnvMapSize
            let fomegap = 4.0 * std::f32::consts::PI / env_map_area;

            // Original paper suggest biasing the mip to improve the results
            let fmipbias = 1.0;
            (0.5 * (fomegas / fomegap).log2() + fmipbias).max(0.0)
        }

        let env_map_area = 6.0 * (PREFILTERED_ENV_MAP_DIM * PREFILTERED_ENV_MAP_DIM) as f32;

        for mip_level in 0..mip_count {
            let mut view = buff.write(mip_level).unwrap();
            let ubo = &mut bytemuck::cast_slice_mut::<_, GpuEnvPrefilterInfo>(view.deref_mut())[0];

            let roughness = mip_level as f32 / (mip_count as f32 - 1.0);

            let mut total_sample_weight = 0.0;

            for i in 0..ENV_PREFILTER_SAMPLE_COUNT {
                let xi = hammersley(i as u32, ENV_PREFILTER_SAMPLE_COUNT as u32);
                let h = halfway_vector(roughness, xi);
                ubo.halfway_vectors[i] = Vec4::from((h, 0.0));
                ubo.mip_levels[i] = compute_mip_level(h, roughness, env_map_area);
                ubo.sample_weights[i] = h.z.max(0.0);
                total_sample_weight += h.z.max(0.0);
            }

            ubo.inv_total_sample_weight = 1.0 / total_sample_weight;
        }

        buff
    }
}
