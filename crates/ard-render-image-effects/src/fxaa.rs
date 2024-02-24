use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_si::bindings::*;
use ordered_float::NotNan;

pub struct Fxaa {
    layout: DescriptorSetLayout,
    pipeline: GraphicsPipeline,
}

pub struct FxaaSets {
    sets: Vec<DescriptorSet>,
}

const SAMPLER: Sampler = Sampler {
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
    border_color: None,
    unnormalize_coords: false,
};

impl Fxaa {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let vert_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./graphics_effect.vert.spv")),
                debug_name: Some("fxaa_vertex_shader".into()),
            },
        )
        .unwrap();

        let frag_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./fxaa.frag.spv")),
                debug_name: Some("fxaa_fragment_shader".into()),
            },
        )
        .unwrap();

        let pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vert_module.clone(),
                    fragment: Some(frag_module),
                },
                layouts: vec![layouts.fxaa.clone()],
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
                debug_name: Some("fxaa_pipeline".into()),
            },
        )
        .unwrap();

        Self {
            layout: layouts.fxaa.clone(),
            pipeline,
        }
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        sets: &'a FxaaSets,
        dst: &'a Texture,
        dst_array_element: usize,
    ) {
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: dst,
                        array_element: dst_array_element,
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
            Some("FXAA"),
            |pass| {
                pass.bind_pipeline(self.pipeline.clone());
                pass.bind_sets(0, vec![&sets.sets[usize::from(frame)]]);
                pass.draw(3, 1, 0, 0);
            },
        );
    }
}

impl FxaaSets {
    pub fn new(ctx: &Context, fxaa: &Fxaa, frames_in_flight: usize) -> Self {
        Self {
            sets: (0..frames_in_flight)
                .map(|i| {
                    DescriptorSet::new(
                        ctx.clone(),
                        DescriptorSetCreateInfo {
                            layout: fxaa.layout.clone(),
                            debug_name: Some(format!("fxaa_set_{i}")),
                        },
                    )
                    .unwrap()
                })
                .collect(),
        }
    }

    pub fn update(&mut self, frame: Frame, src: &Texture, src_array_element: usize) {
        self.sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: FXAA_SET_INPUT_TEXTURE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src,
                array_element: src_array_element,
                sampler: SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }
}
