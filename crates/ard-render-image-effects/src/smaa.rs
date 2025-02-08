use ard_ecs::resource::Resource;
use ard_math::{UVec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{Frame, FRAMES_IN_FLIGHT};
use ard_render_si::{bindings::*, types::*};
use image::GenericImageView;
use ordered_float::NotNan;

#[derive(Copy, Clone, Resource)]
pub struct SmaaSettings {
    pub enabled: bool,
    pub edge_visualization: bool,
}

pub struct Smaa {
    _search_tex: Texture,
    _area_tex: Texture,
    edges_tex: Texture,
    blend_tex: Texture,
    detected_edges: Buffer,
    edges_indirect: Buffer,
    reset_edges_pipeline: ComputePipeline,
    reset_edges_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    edges_pipeline: ComputePipeline,
    edges_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    weights_pipeline: ComputePipeline,
    weights_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    blend_pipeline: GraphicsPipeline,
    blend_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

const WORK_GROUP_SIZE_1D: u32 = 64;
const WORK_GROUP_SIZE_2D: u32 = 8;

pub const SMAA_SAMPLER: Sampler = Sampler {
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

impl Default for SmaaSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            edge_visualization: false,
        }
    }
}

impl Smaa {
    pub fn new(ctx: &Context, layouts: &Layouts, dims: (u32, u32)) -> Self {
        let search_tex_image = image::load_from_memory_with_format(
            include_bytes!("../bin/smaa/search_tex.png"),
            image::ImageFormat::Png,
        )
        .unwrap();
        let (width, height) = search_tex_image.dimensions();
        let bytes = search_tex_image.to_rgba8().to_vec();
        let search_tex_staging = Buffer::new_staging(
            ctx.clone(),
            QueueType::Main,
            Some("smaa_search_tex_staging".into()),
            &bytes,
        )
        .unwrap();

        let search_tex = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_search_tex".into()),
            },
        )
        .unwrap();

        let area_tex_image = image::load_from_memory_with_format(
            include_bytes!("../bin/smaa/area_tex.png"),
            image::ImageFormat::Png,
        )
        .unwrap();
        let (width, height) = area_tex_image.dimensions();
        let bytes = area_tex_image.to_rgba8().to_vec();
        let area_tex_staging = Buffer::new_staging(
            ctx.clone(),
            QueueType::Main,
            Some("smaa_area_tex_staging".into()),
            &bytes,
        )
        .unwrap();

        let area_tex = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_area_tex".into()),
            },
        )
        .unwrap();

        let mut cb = ctx.main().command_buffer();
        cb.copy_buffer_to_texture(
            &search_tex,
            &search_tex_staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: search_tex.dims(),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        cb.copy_buffer_to_texture(
            &area_tex,
            &area_tex_staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: area_tex.dims(),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        ctx.main().submit(Some("smaa_tex_upload"), cb);

        let edges_tex = Self::create_edges_tex(ctx, dims);
        let blend_tex = Self::create_blend_tex(ctx, dims);
        let detected_edges = Self::create_detected_edges_buf(ctx, dims);
        let edges_indirect = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<DispatchIndirect>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_edges_indirect".into()),
            },
        )
        .unwrap();

        let reset_edges_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.smaa_reset_edges.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(
                            env!("OUT_DIR"),
                            "./smaa_reset_edges.comp.spv"
                        )),
                        debug_name: Some("smaa_reset_edges_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (1, 1, 1),
                push_constants_size: None,
                debug_name: Some("smaa_reset_edges_pipeline".into()),
            },
        )
        .unwrap();

        let edges_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.smaa_edge_detect.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(env!("OUT_DIR"), "./smaa_edges.comp.spv")),
                        debug_name: Some("smaa_edges_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE_2D, WORK_GROUP_SIZE_2D, 1),
                push_constants_size: Some(std::mem::size_of::<GpuSmaaPushConstants>() as u32),
                debug_name: Some("smaa_edges_pipeline".into()),
            },
        )
        .unwrap();

        let weights_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.smaa_weights.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(env!("OUT_DIR"), "./smaa_weights.comp.spv")),
                        debug_name: Some("smaa_weights_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE_1D, 1, 1),
                push_constants_size: Some(std::mem::size_of::<GpuSmaaPushConstants>() as u32),
                debug_name: Some("smaa_weights_pipeline".into()),
            },
        )
        .unwrap();

        let blend_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(env!("OUT_DIR"), "./smaa_blend.vert.spv")),
                            debug_name: Some("smaa_blend_vertex_shader".into()),
                        },
                    )
                    .unwrap(),
                    fragment: Some(
                        Shader::new(
                            ctx.clone(),
                            ShaderCreateInfo {
                                code: include_bytes!(concat!(
                                    env!("OUT_DIR"),
                                    "./smaa_blend.frag.spv"
                                )),
                                debug_name: Some("smaa_blend_fragment_shader".into()),
                            },
                        )
                        .unwrap(),
                    ),
                },
                layouts: vec![layouts.smaa_blend.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::CounterClockwise,
                    alpha_to_coverage: false,
                },
                depth_stencil: None,
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        blend: false,
                        ..Default::default()
                    }],
                },
                push_constants_size: Some(std::mem::size_of::<GpuSmaaPushConstants>() as u32),
                debug_name: Some("smaa_blend_pipeline".into()),
            },
        )
        .unwrap();

        let reset_edges_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.smaa_reset_edges.clone(),
                    debug_name: Some("smaa_reset_edges_set".into()),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: SMAA_RESET_EDGES_SET_INDIRECT_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &edges_indirect,
                    array_element: 0,
                },
            }]);

            set
        });

        let edges_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.smaa_edge_detect.clone(),
                    debug_name: Some("smaa_edge_detect_set".into()),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: SMAA_EDGE_DETECT_SET_INDIRECT_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &edges_indirect,
                    array_element: 0,
                },
            }]);

            set
        });

        let weights_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.smaa_weights.clone(),
                    debug_name: Some("smaa_weights_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: SMAA_WEIGHTS_SET_SEARCH_TEX_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &search_tex,
                        array_element: 0,
                        sampler: SMAA_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                },
                DescriptorSetUpdate {
                    binding: SMAA_WEIGHTS_SET_AREA_TEX_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &area_tex,
                        array_element: 0,
                        sampler: SMAA_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                },
            ]);

            set
        });

        let blend_sets = std::array::from_fn(|_| {
            DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.smaa_blend.clone(),
                    debug_name: Some("smaa_blend_set".into()),
                },
            )
            .unwrap()
        });

        Self {
            _search_tex: search_tex,
            _area_tex: area_tex,
            edges_tex,
            blend_tex,
            detected_edges,
            edges_indirect,
            reset_edges_pipeline,
            reset_edges_sets,
            edges_pipeline,
            edges_sets,
            weights_pipeline,
            weights_sets,
            blend_pipeline,
            blend_sets,
        }
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.edges_tex = Self::create_edges_tex(ctx, dims);
        self.blend_tex = Self::create_blend_tex(ctx, dims);
        self.detected_edges = Self::create_detected_edges_buf(ctx, dims);
    }

    pub fn update_bindings(&mut self, frame: Frame, src: &Texture) {
        let frame = usize::from(frame);

        self.reset_edges_sets[frame].update(&[DescriptorSetUpdate {
            binding: SMAA_RESET_EDGES_SET_EDGES_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: &self.detected_edges,
                array_element: 0,
            },
        }]);

        self.edges_sets[frame].update(&[
            DescriptorSetUpdate {
                binding: SMAA_EDGE_DETECT_SET_EDGE_BUFFER_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.detected_edges,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SMAA_EDGE_DETECT_SET_EDGE_TEXTURE_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.edges_tex,
                    array_element: 0,
                    mip: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SMAA_EDGE_DETECT_SET_SOURCE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: src,
                    array_element: 0,
                    sampler: SMAA_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);

        self.weights_sets[frame].update(&[
            DescriptorSetUpdate {
                binding: SMAA_WEIGHTS_SET_EDGE_BUFFER_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.detected_edges,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SMAA_WEIGHTS_SET_EDGE_TEXTURE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &self.edges_tex,
                    array_element: 0,
                    sampler: SMAA_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: SMAA_WEIGHTS_SET_BLEND_TEXTURE_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.blend_tex,
                    mip: 0,
                    array_element: 0,
                },
            },
        ]);

        self.blend_sets[frame].update(&[
            DescriptorSetUpdate {
                binding: SMAA_BLEND_SET_SOURCE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: src,
                    array_element: 0,
                    sampler: SMAA_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: SMAA_BLEND_SET_BLEND_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &self.blend_tex,
                    array_element: 0,
                    sampler: SMAA_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        dst: ColorAttachmentDestination<'a>,
        edges_only: bool,
    ) {
        let frame = usize::from(frame);

        let (width, height, _) = self.blend_tex.dims();

        let consts = [GpuSmaaPushConstants {
            rt_metrics: Vec4::new(
                1.0 / width as f32,
                1.0 / height as f32,
                width as f32,
                height as f32,
            ),
            screen_dims: UVec2::new(width, height),
            edge_viz: edges_only as u32,
        }];

        // Reset edge counter and indirect dispatch buffer
        commands.compute_pass(
            &self.reset_edges_pipeline,
            Some("smaa_reset_edges"),
            |pass| {
                pass.bind_sets(0, vec![&self.reset_edges_sets[frame]]);
                ComputePassDispatch::Inline(1, 1, 1)
            },
        );

        // Detect edges
        commands.compute_pass(&self.edges_pipeline, Some("smaa_edge_detect"), |pass| {
            pass.bind_sets(0, vec![&self.edges_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&consts));
            ComputePassDispatch::Inline(
                width.div_ceil(WORK_GROUP_SIZE_2D),
                height.div_ceil(WORK_GROUP_SIZE_2D),
                1,
            )
        });

        // Reset blend texture
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: &self.blend_tex,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                depth_stencil_attachment: None,
                color_resolve_attachments: Vec::default(),
                depth_stencil_resolve_attachment: None,
            },
            Some("smaa_blend_reset"),
            |_| {},
        );

        // Create blend texture
        commands.compute_pass(&self.weights_pipeline, Some("smaa_weights"), |pass| {
            pass.bind_sets(0, vec![&self.weights_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&consts));
            ComputePassDispatch::Indirect {
                buffer: &self.edges_indirect,
                array_element: 0,
                offset: 0,
            }
        });

        // Perform final blending onto the surface
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst,
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                depth_stencil_attachment: None,
                color_resolve_attachments: Vec::default(),
                depth_stencil_resolve_attachment: None,
            },
            Some("smaa_blend"),
            |pass| {
                pass.bind_pipeline(self.blend_pipeline.clone());
                pass.bind_sets(0, vec![&self.blend_sets[frame]]);
                pass.push_constants(bytemuck::cast_slice(&consts));
                pass.draw(3, 1, 0, 0);
            },
        );
    }

    fn create_edges_tex(ctx: &Context, dims: (u32, u32)) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rg8Unorm,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_edges_tex".into()),
            },
        )
        .unwrap()
    }

    fn create_blend_tex(ctx: &Context, dims: (u32, u32)) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::STORAGE
                    | TextureUsage::SAMPLED
                    | TextureUsage::COLOR_ATTACHMENT,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_blend_tex".into()),
            },
        )
        .unwrap()
    }

    fn create_detected_edges_buf(ctx: &Context, dims: (u32, u32)) -> Buffer {
        Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (std::mem::size_of::<u32>() as u32 * (dims.0 * dims.1 + 1)) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("smaa_detected_edges".into()),
            },
        )
        .unwrap()
    }
}
