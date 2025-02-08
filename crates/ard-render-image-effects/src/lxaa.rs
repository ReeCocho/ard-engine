use ard_ecs::prelude::*;
use ard_math::{UVec2, Vec2};
use ard_pal::prelude::*;
use ard_render_base::{Frame, FRAMES_IN_FLIGHT};
use ard_render_si::{bindings::*, types::*};
use ordered_float::NotNan;

#[derive(Copy, Clone, Resource)]
pub struct LxaaSettings {
    pub enabled: bool,
}

pub struct Lxaa {
    pipeline: GraphicsPipeline,
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    dims: (u32, u32),
}

const LXAA_SAMPLER: Sampler = Sampler {
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

impl Default for LxaaSettings {
    fn default() -> Self {
        Self { enabled: false }
    }
}

impl Lxaa {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./graphics_effect.vert.spv"
                            )),
                            debug_name: Some("lxaa_vertex_shader".into()),
                        },
                    )
                    .unwrap(),
                    fragment: Some(
                        Shader::new(
                            ctx.clone(),
                            ShaderCreateInfo {
                                code: include_bytes!(concat!(env!("OUT_DIR"), "./lxaa.frag.spv")),
                                debug_name: Some("lxaa_fragment_shader".into()),
                            },
                        )
                        .unwrap(),
                    ),
                },
                layouts: vec![layouts.lxaa.clone()],
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
                push_constants_size: Some(std::mem::size_of::<GpuLxaaPushConstants>() as u32),
                debug_name: Some("lxaa_pipeline".into()),
            },
        )
        .unwrap();

        let sets = std::array::from_fn(|_| {
            DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.lxaa.clone(),
                    debug_name: Some("lxaa_set".into()),
                },
            )
            .unwrap()
        });

        Self {
            pipeline,
            sets,
            dims: (0, 0),
        }
    }

    pub fn update_bindings(&mut self, frame: Frame, src: (&Texture, usize)) {
        let frame = usize::from(frame);

        self.dims.0 = src.0.dims().0;
        self.dims.1 = src.0.dims().1;

        self.sets[frame].update(&[DescriptorSetUpdate {
            binding: LXAA_SET_SRC_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src.0,
                array_element: src.1,
                sampler: LXAA_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        dst: ColorAttachmentDestination<'a>,
    ) {
        assert_ne!(self.dims.0, 0);
        assert_ne!(self.dims.1, 0);

        let frame = usize::from(frame);

        let consts = [GpuLxaaPushConstants {
            screen_dims: UVec2::new(self.dims.0, self.dims.1),
            inv_screen_dims: Vec2::new(1.0 / self.dims.0 as f32, 1.0 / self.dims.1 as f32),
        }];

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
            Some("lxaa"),
            |pass| {
                pass.bind_pipeline(self.pipeline.clone());
                pass.bind_sets(0, vec![&self.sets[frame]]);
                pass.push_constants(bytemuck::cast_slice(&consts));
                pass.draw(3, 1, 0, 0);
            },
        );
    }
}
