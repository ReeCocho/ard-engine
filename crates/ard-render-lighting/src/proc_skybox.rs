use ard_pal::prelude::*;
use ard_render_si::bindings::Layouts;

pub struct ProceduralSkyBox {
    pipeline: GraphicsPipeline,
}

impl ProceduralSkyBox {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
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

        let pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vs,
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

        Self { pipeline }
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut RenderPass<'a>,
        camera_set: &'a DescriptorSet,
        global_set: &'a DescriptorSet,
    ) {
        pass.bind_pipeline(self.pipeline.clone());
        pass.bind_sets(0, vec![camera_set, global_set]);
        pass.draw(36, 1, 0, 0);
    }
}
