use ard_pal::prelude::*;

use crate::{
    factory::{
        allocator::{EscapeHandle, ResourceId},
        materials::{MaterialBlock, MaterialBuffers},
        Factory, Layouts,
    },
    mesh::VertexLayout,
};

pub struct MaterialCreateInfo {
    pub vertex_shader: Shader,
    pub fragment_shader: Shader,
    pub vertex_layout: VertexLayout,
    pub texture_count: usize,
    pub data_size: u64,
}

pub struct MaterialInstanceCreateInfo {
    pub material: Material,
}

#[derive(Clone)]
pub struct Material {
    pub(crate) id: ResourceId,
    pub(crate) data_size: u64,
    pub(crate) texture_count: usize,
    pub(crate) escaper: EscapeHandle,
}

#[derive(Clone)]
pub struct MaterialInstance {
    pub(crate) id: ResourceId,
    pub(crate) material: Material,
    pub(crate) factory: Factory,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct MaterialInner {
    pub pipelines: Pipelines,
}

pub(crate) struct MaterialInstanceInner {
    pub data: Vec<u8>,
    pub material_block: Option<MaterialBlock>,
}

pub(crate) struct Pipelines {
    pub depth_only: GraphicsPipeline,
    pub opaque: GraphicsPipeline,
    pub shadow: GraphicsPipeline,
}

impl MaterialInner {
    pub fn new(ctx: &Context, create_info: MaterialCreateInfo, layouts: &Layouts) -> Self {
        let vertex_input = create_info.vertex_layout.vertex_input_state();

        let depth_only = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: create_info.vertex_shader.clone(),
                    fragment: None,
                },
                layouts: vec![layouts.global.clone(), layouts.materials.clone()],
                vertex_input: vertex_input.clone(),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: None,
                debug_name: None,
            },
        )
        .unwrap();

        let shadow = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: create_info.vertex_shader.clone(),
                    fragment: None,
                },
                layouts: vec![layouts.global.clone(), layouts.materials.clone()],
                vertex_input: vertex_input.clone(),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: true,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: None,
                debug_name: None,
            },
        )
        .unwrap();

        let opaque = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: create_info.vertex_shader.clone(),
                    fragment: Some(create_info.fragment_shader.clone()),
                },
                layouts: vec![layouts.global.clone(), layouts.materials.clone()],
                vertex_input: vertex_input.clone(),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: Some(ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: false,
                        ..Default::default()
                    }],
                }),
                debug_name: None,
            },
        )
        .unwrap();

        MaterialInner {
            pipelines: Pipelines {
                depth_only,
                opaque,
                shadow,
            },
        }
    }
}

impl MaterialInstanceInner {
    pub fn new(
        material_buffers: &mut MaterialBuffers,
        create_info: MaterialInstanceCreateInfo,
    ) -> Self {
        let material_block = if create_info.material.data_size > 0 {
            Some(material_buffers.allocate_ubo(create_info.material.data_size))
        } else {
            None
        };

        MaterialInstanceInner {
            data: vec![0; create_info.material.data_size as usize],
            material_block,
        }
    }
}
