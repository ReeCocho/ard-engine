use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{
    ColorBlendAttachment, ColorBlendState, ColorComponents, CompareOp, CullMode, DepthStencilState,
    FrontFace, PolygonMode, RasterizationState,
};
use ard_render_material::{
    material::{Material, MaterialCreateInfo, MaterialVariantDescriptor},
    shader::{Shader, ShaderCreateInfo},
};
use ard_render_renderers::{DEPTH_PREPASS_PASS_ID, HIGH_Z_PASS_ID, OPAQUE_PASS_ID};
use ard_render_si::types::GpuPbrMaterial;

pub type PbrMaterialData = GpuPbrMaterial;

/// Creates the PBR material given functions that can create shader modules and materials (this is
/// probably going to be a wrapper for the factories shader creation function).
pub fn create_pbr_material(
    create_shader: impl Fn(ShaderCreateInfo) -> Shader,
    create_material: impl Fn(MaterialCreateInfo) -> Material,
) -> Material {
    // Initialize shaders
    let simple_vertex_shader = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.spv")),
        debug_name: Some("pbr_simple_vertex".into()),
        texture_slots: 0,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let simple_fragment_shader = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.spv")),
        debug_name: Some("pbr_simple_fragment".into()),
        texture_slots: 0,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    // Actual material creation info
    create_material(MaterialCreateInfo {
        variants: vec![
            // Depth prepass for untextured materials.
            MaterialVariantDescriptor {
                pass_id: DEPTH_PREPASS_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: simple_vertex_shader.clone(),
                fragment_shader: None,
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Greater,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState::default(),
                debug_name: Some("pbr_depth_prepass_pipeline(Position, Normal)".into()),
            },
            // Opaque pass for simplest geometry type.
            MaterialVariantDescriptor {
                pass_id: OPAQUE_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: simple_vertex_shader.clone(),
                fragment_shader: Some(simple_fragment_shader),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
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
                debug_name: Some("pbr_opaque_pipeline(Position, Normal)".into()),
            },
            // HZB pass.
            MaterialVariantDescriptor {
                pass_id: HIGH_Z_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: simple_vertex_shader,
                fragment_shader: None,
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Greater,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState::default(),
                debug_name: Some("pbr_hzb_pass_pipeline(Position, Normal)".into()),
            }, // TODO: Depth prepass for alpha masked instances.
               // TODO: Opaque pass for textured instances.
               // TODO: Depth only pass for opaque instances.
               // TOOD: Shadow pass for opaque instances.
               // TODO: Transparent pass for untextured instances.
               // TODO: Transparent pass for textured instances.
        ],
        data_size: std::mem::size_of::<GpuPbrMaterial>() as u32,
        texture_slots: 0,
    })
}
