use std::mem::offset_of;

use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_debug::{buffer::DebugVertexBuffer, shape::DebugShapeVertex};
use ard_render_si::bindings::Layouts;

pub struct DebugRenderer {
    pipeline: GraphicsPipeline,
}

impl DebugRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let vertex = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./debug.vert.spv")),
                debug_name: Some("debug_vertex_shader".into()),
            },
        )
        .unwrap();

        let fragment = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./debug.frag.spv")),
                debug_name: Some("debug_fragment_shader".into()),
            },
        )
        .unwrap();

        let pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex,
                    fragment: Some(fragment),
                },
                layouts: vec![layouts.camera.clone()],
                vertex_input: VertexInputState {
                    attributes: vec![
                        VertexInputAttribute {
                            binding: 0,
                            location: 0,
                            format: Format::Rgba32SFloat,
                            offset: offset_of!(DebugShapeVertex, position) as u32,
                        },
                        VertexInputAttribute {
                            binding: 0,
                            location: 1,
                            format: Format::Rgba32SFloat,
                            offset: offset_of!(DebugShapeVertex, color) as u32,
                        },
                    ],
                    bindings: vec![VertexInputBinding {
                        binding: 0,
                        stride: std::mem::size_of::<DebugShapeVertex>() as u32,
                        input_rate: VertexInputRate::Vertex,
                    }],
                    topology: PrimitiveTopology::LineList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Line,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: None,
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: true,
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        color_blend_op: BlendOp::Add,
                        src_color_blend_factor: BlendFactor::SrcAlpha,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        alpha_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::One,
                        dst_alpha_blend_factor: BlendFactor::Zero,
                    }],
                },
                push_constants_size: None,
                debug_name: Some("debug_drawing_pipeline".into()),
            },
        )
        .unwrap();

        Self { pipeline }
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        pass: &mut RenderPass<'a>,
        vertices: &'a DebugVertexBuffer,
        camera: &'a CameraUbo,
    ) {
        pass.bind_pipeline(self.pipeline.clone());
        pass.bind_sets(0, vec![camera.get_set(frame)]);
        pass.bind_vertex_buffers(
            0,
            vec![VertexBind {
                buffer: vertices.buffer(),
                array_element: 0,
                offset: 0,
            }],
        );
        pass.draw(vertices.vertex_count(), 1, 0, 0);
    }
}
