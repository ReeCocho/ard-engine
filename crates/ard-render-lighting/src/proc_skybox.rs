use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, consts::*, types::*};
use ordered_float::NotNan;

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
    /// Pipeline for gathering diffuse irradiance spherical harmonic coefficients.
    di_gather_pipeline: ComputePipeline,
    di_gather_set: DescriptorSet,
    /// Pipeline for summing all coefficients together.
    di_par_reduce_pipeline: ComputePipeline,
    di_par_reduce_set: DescriptorSet,
    // Pipeline for rendering the diffuse irradiance map.
    di_render_pipeline: GraphicsPipeline,
    di_render_set: DescriptorSet,
    // Camera UBO for rendering the diffuse irradiance map.
    di_render_camera: CameraUbo,
    /// Samples for computing diffuse irradiance.
    _diffuse_irradiance_samples: Buffer,
    // Prefiltering matrices. Output from irradiance gather pipeline.
    _prefiltering_matrices: Buffer,
    // Prefiltered diffuse irradiance map.
    di_map: CubeMap,
}

impl ProceduralSkyBox {
    pub fn new(ctx: &Context, layouts: &Layouts, fif: usize) -> Self {
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

        // Prefiltering diffuse irradiance map.
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

        // Camera for rendering the diffuse irradiance map
        let mut di_render_camera = CameraUbo::new(ctx, fif, false, layouts);

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
                    view_inv: view.inverse(),
                    projection_inv: proj.inverse(),
                    vp_inv: vp.inverse(),
                    frustum,
                    position: Vec4::from((Vec3::ZERO, 1.0)),
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
                stages: ShaderStages {
                    vertex: vs.clone(),
                    fragment: Some(fs),
                },
                layouts: vec![layouts.camera.clone(), layouts.global.clone()],
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
                push_constants_size: None,
                debug_name: Some(String::from("sky_box_pipeline")),
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
                stages: ShaderStages {
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

        Self {
            sky_box_pipeline,
            _diffuse_irradiance_samples: diffuse_irradiance_samples,
            di_gather_pipeline,
            di_gather_set,
            di_par_reduce_pipeline,
            di_par_reduce_set,
            di_render_pipeline,
            di_render_camera,
            di_render_set,
            _prefiltering_matrices: prefiltering_matrices,
            di_map,
        }
    }

    #[inline(always)]
    pub fn di_map(&self) -> &CubeMap {
        &self.di_map
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

    pub fn render<'a>(
        &'a self,
        pass: &mut RenderPass<'a>,
        camera_set: &'a DescriptorSet,
        global_set: &'a DescriptorSet,
    ) {
        pass.bind_pipeline(self.sky_box_pipeline.clone());
        pass.bind_sets(0, vec![camera_set, global_set]);
        pass.draw(36, 1, 0, 0);
    }

    const fn diffuse_irradiance_buffer_size() -> u64 {
        // For each sample...
        (DIFFUSE_IRRADIANCE_SAMPLE_DIM * DIFFUSE_IRRADIANCE_SAMPLE_DIM) as u64
        // We have 9 values for our spherical harmonics and each value has three color channels,
        // making 27 values total, but we round up to 28 so we can use 7 Vec4s instead of floats.
        * 28 * std::mem::size_of::<f32>() as u64
    }
}
