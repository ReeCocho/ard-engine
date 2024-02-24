use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_si::bindings::*;
use ordered_float::NotNan;

const BLOOM_IMAGE_FORMAT: Format = Format::Rgba16SFloat;

pub const BLOOM_SAMPLE_FILTER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: None,
    unnormalize_coords: false,
};

pub struct Bloom<const FIF: usize> {
    bloom_image: Texture,
    layout: DescriptorSetLayout,
    downscale: GraphicsPipeline,
    downscale_sets: [Vec<DescriptorSet>; FIF],
    upscale: GraphicsPipeline,
    upscale_sets: [Vec<DescriptorSet>; FIF],
}

impl<const FIF: usize> Bloom<FIF> {
    pub fn new(ctx: &Context, layouts: &Layouts, dims: (u32, u32), mip_count: usize) -> Self {
        let bloom_image = Self::make_bloom_image(ctx, dims, mip_count);

        let vert_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./graphics_effect.vert.spv")),
                debug_name: Some("tonemapping_vertex_shader".into()),
            },
        )
        .unwrap();

        let frag_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./bloom_downscale.frag.spv")),
                debug_name: Some("bloom_downscale_fragment_shader".into()),
            },
        )
        .unwrap();

        let downscale = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vert_module.clone(),
                    fragment: Some(frag_module),
                },
                layouts: vec![layouts.bloom.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::CounterClockwise,
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
                push_constants_size: None,
                debug_name: Some("bloom_downscale_pipeline".into()),
            },
        )
        .unwrap();

        let frag_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./bloom_upscale.frag.spv")),
                debug_name: Some("bloom_upscale_fragment_shader".into()),
            },
        )
        .unwrap();

        let upscale = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vert_module.clone(),
                    fragment: Some(frag_module),
                },
                layouts: vec![layouts.bloom.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::CounterClockwise,
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
                push_constants_size: None,
                debug_name: Some("bloom_upscale_pipeline".into()),
            },
        )
        .unwrap();

        let downscale_sets = std::array::from_fn(|_| {
            Self::make_downscale_sets(ctx, &layouts.bloom, &bloom_image, dims, mip_count)
        });

        let upscale_sets = std::array::from_fn(|_| {
            Self::make_upscale_sets(ctx, &layouts.bloom, &bloom_image, dims, mip_count)
        });

        Self {
            bloom_image,
            layout: layouts.bloom.clone(),
            downscale,
            downscale_sets,
            upscale,
            upscale_sets,
        }
    }

    #[inline(always)]
    pub fn image(&self) -> &Texture {
        &self.bloom_image
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32), mip_count: usize) {
        self.bloom_image = Self::make_bloom_image(ctx, dims, mip_count);

        self.downscale_sets = std::array::from_fn(|_| {
            Self::make_downscale_sets(ctx, &self.layout, &self.bloom_image, dims, mip_count)
        });

        self.upscale_sets = std::array::from_fn(|_| {
            Self::make_upscale_sets(ctx, &self.layout, &self.bloom_image, dims, mip_count)
        });
    }

    fn mip_count(dims: (u32, u32), mip_count: usize) -> usize {
        let half_dims = (dims.0 / 2, dims.1 / 2);
        half_dims
            .0
            .max(half_dims.1)
            .checked_ilog2()
            .unwrap_or(1)
            .min(mip_count as u32) as usize
    }

    fn make_bloom_image(ctx: &Context, dims: (u32, u32), mip_count: usize) -> Texture {
        // Compute the actual valid mip count based on screen dimensions
        let half_dims = (dims.0 / 2, dims.1 / 2);
        let mip_count = Self::mip_count(dims, mip_count);

        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: BLOOM_IMAGE_FORMAT,
                ty: TextureType::Type2D,
                width: half_dims.0,
                height: half_dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: mip_count as usize,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("bloom_image".into()),
            },
        )
        .unwrap()
    }

    fn make_downscale_sets(
        ctx: &Context,
        layout: &DescriptorSetLayout,
        bloom_image: &Texture,
        dims: (u32, u32),
        mip_count: usize,
    ) -> Vec<DescriptorSet> {
        let mip_count = Self::mip_count(dims, mip_count);

        let sets = (0..mip_count)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layout.clone(),
                        debug_name: Some(format!("bloom_downscale_set_{i}")),
                    },
                )
                .unwrap();

                if i > 0 {
                    set.update(&[DescriptorSetUpdate {
                        binding: BLOOM_SET_SOURCE_IMAGE_BINDING,
                        array_element: 0,
                        value: DescriptorValue::Texture {
                            texture: bloom_image,
                            array_element: 0,
                            sampler: BLOOM_SAMPLE_FILTER,
                            base_mip: i - 1,
                            mip_count: 1,
                        },
                    }]);
                }

                set
            })
            .collect();

        sets
    }

    fn make_upscale_sets(
        ctx: &Context,
        layout: &DescriptorSetLayout,
        bloom_image: &Texture,
        dims: (u32, u32),
        mip_count: usize,
    ) -> Vec<DescriptorSet> {
        let mip_count = Self::mip_count(dims, mip_count) - 1;

        let sets = (0..mip_count)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layout.clone(),
                        debug_name: Some(format!("bloom_upscale_set_{i}")),
                    },
                )
                .unwrap();

                set.update(&[DescriptorSetUpdate {
                    binding: BLOOM_SET_SOURCE_IMAGE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: bloom_image,
                        array_element: 0,
                        sampler: BLOOM_SAMPLE_FILTER,
                        base_mip: i + 1,
                        mip_count: 1,
                    },
                }]);

                set
            })
            .collect();

        sets
    }

    pub fn bind_images(&mut self, frame: Frame, src: &Texture) {
        self.downscale_sets[usize::from(frame)][0].update(&[DescriptorSetUpdate {
            binding: BLOOM_SET_SOURCE_IMAGE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src,
                array_element: 0,
                sampler: BLOOM_SAMPLE_FILTER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn render<'a>(&'a self, frame: Frame, commands: &mut CommandBuffer<'a>) {
        // Perform downscaling
        self.downscale_sets[usize::from(frame)]
            .iter()
            .enumerate()
            .for_each(|(mip, set)| {
                commands.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            dst: ColorAttachmentDestination::Texture {
                                texture: &self.bloom_image,
                                array_element: 0,
                                mip_level: mip,
                            },
                            load_op: LoadOp::DontCare,
                            store_op: StoreOp::Store,
                            samples: MultiSamples::Count1,
                        }],
                        depth_stencil_attachment: None,
                        color_resolve_attachments: Vec::default(),
                        depth_stencil_resolve_attachment: None,
                    },
                    Some("bloom_downscale"),
                    |pass| {
                        pass.bind_pipeline(self.downscale.clone());
                        pass.bind_sets(0, vec![set]);
                        pass.draw(3, 1, 0, 0);
                    },
                );
            });

        // Perform upscaling
        self.upscale_sets[usize::from(frame)]
            .iter()
            .enumerate()
            .rev()
            .for_each(|(mip, set)| {
                commands.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            dst: ColorAttachmentDestination::Texture {
                                texture: &self.bloom_image,
                                array_element: 0,
                                mip_level: mip,
                            },
                            load_op: LoadOp::DontCare,
                            store_op: StoreOp::Store,
                            samples: MultiSamples::Count1,
                        }],
                        depth_stencil_attachment: None,
                        color_resolve_attachments: Vec::default(),
                        depth_stencil_resolve_attachment: None,
                    },
                    Some("bloom_upscale"),
                    |pass| {
                        pass.bind_pipeline(self.upscale.clone());
                        pass.bind_sets(0, vec![set]);
                        pass.draw(3, 1, 0, 0);
                    },
                );
            });
    }
}
