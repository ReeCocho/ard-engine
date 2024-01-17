use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{
    BlendFactor, BlendOp, ColorBlendAttachment, ColorBlendState, ColorComponents, CompareOp,
    CullMode, DepthStencilState, FrontFace, PolygonMode, RasterizationState,
};
use ard_render_material::{
    material::{Material, MaterialCreateInfo, MaterialVariantDescriptor},
    material_instance::TextureSlot,
    shader::{Shader, ShaderCreateInfo},
};
use ard_render_renderers::{
    DEPTH_PREPASS_PASS_ID, HIGH_Z_PASS_ID, OPAQUE_PASS_ID, SHADOW_PASS_ID, TRANSPARENT_PASS_ID,
};
use ard_render_si::types::GpuPbrMaterial;

pub type PbrMaterialData = GpuPbrMaterial;

pub const PBR_MATERIAL_TEXTURE_COUNT: usize = 3;
pub const PBR_MATERIAL_DIFFUSE_SLOT: TextureSlot = TextureSlot(0);
pub const PBR_MATERIAL_NORMAL_SLOT: TextureSlot = TextureSlot(1);
pub const PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT: TextureSlot = TextureSlot(2);

/// Creates the PBR material given functions that can create shader modules and materials (this is
/// probably going to be a wrapper for the factories shader creation function).
pub fn create_pbr_material(
    create_shader: impl Fn(ShaderCreateInfo) -> Shader,
    create_material: impl Fn(MaterialCreateInfo) -> Material,
) -> Material {
    // Initialize shaders
    let vertex_shader = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.spv")),
        debug_name: Some("pbr_vertex".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let fragment_shader = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.spv")),
        debug_name: Some("pbr_fragment".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let vertex_shader_tuv0 = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.tuv0.spv")),
        debug_name: Some("pbr_vertex_tuv0".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let fragment_shader_tuv0 = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.tuv0.spv")),
        debug_name: Some("pbr_fragment_tuv0".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let vertex_shader_uv0d = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.uv0d.spv")),
        debug_name: Some("pbr_vertex_uv0d".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    let fragment_shader_uv0d = create_shader(ShaderCreateInfo {
        code: include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.uv0d.spv")),
        debug_name: Some("pbr_fragment_uv0d".into()),
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
        data_size: std::mem::size_of::<GpuPbrMaterial>(),
    });

    // Actual material creation info
    create_material(MaterialCreateInfo {
        variants: vec![
            // Depth prepasses
            MaterialVariantDescriptor {
                pass_id: DEPTH_PREPASS_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: vertex_shader.clone(),
                fragment_shader: None,
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
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
            MaterialVariantDescriptor {
                pass_id: DEPTH_PREPASS_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL | VertexLayout::UV0,
                vertex_shader: vertex_shader_uv0d.clone(),
                fragment_shader: Some(fragment_shader_uv0d.clone()),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
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
                debug_name: Some("pbr_depth_prepass_pipeline(Position, Normal, Uv0)".into()),
            },
            // Shadow passes
            MaterialVariantDescriptor {
                pass_id: SHADOW_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: vertex_shader.clone(),
                fragment_shader: None,
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: true,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState::default(),
                debug_name: Some("pbr_shadow_pass_pipeline(Position, Normal)".into()),
            },
            MaterialVariantDescriptor {
                pass_id: SHADOW_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL | VertexLayout::UV0,
                vertex_shader: vertex_shader_uv0d.clone(),
                fragment_shader: Some(fragment_shader_uv0d.clone()),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: true,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState::default(),
                debug_name: Some("pbr_shadow_pass_pipeline(Position, Normal, Uv0)".into()),
            },
            // Opaque passes
            MaterialVariantDescriptor {
                pass_id: OPAQUE_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: vertex_shader.clone(),
                fragment_shader: Some(fragment_shader.clone()),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
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
            MaterialVariantDescriptor {
                pass_id: OPAQUE_PASS_ID,
                vertex_layout: VertexLayout::POSITION
                    | VertexLayout::NORMAL
                    | VertexLayout::TANGENT
                    | VertexLayout::UV0,
                vertex_shader: vertex_shader_tuv0.clone(),
                fragment_shader: Some(fragment_shader_tuv0.clone()),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
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
                debug_name: Some("pbr_opaque_pipeline(Position, Normal, Tangent, Uv0)".into()),
            },
            // Transparency passes
            MaterialVariantDescriptor {
                pass_id: TRANSPARENT_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader: vertex_shader.clone(),
                fragment_shader: Some(fragment_shader),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::GreaterOrEqual,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
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
                debug_name: Some("pbr_transparent_pipeline(Position, Normal)".into()),
            },
            MaterialVariantDescriptor {
                pass_id: TRANSPARENT_PASS_ID,
                vertex_layout: VertexLayout::POSITION
                    | VertexLayout::NORMAL
                    | VertexLayout::TANGENT
                    | VertexLayout::UV0,
                vertex_shader: vertex_shader_tuv0.clone(),
                fragment_shader: Some(fragment_shader_tuv0),
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::GreaterOrEqual,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
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
                debug_name: Some("pbr_transparent_pipeline(Position, Normal)".into()),
            },
            // HZB pass
            MaterialVariantDescriptor {
                pass_id: HIGH_Z_PASS_ID,
                vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL,
                vertex_shader,
                fragment_shader: None,
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
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
            },
            // TODO: Depth prepass for alpha masked instances.
            // TODO: Opaque pass for textured instances.
            // TODO: Depth only pass for opaque instances.
            // TODO: Transparent pass for untextured instances.
            // TODO: Transparent pass for textured instances.
        ],
        data_size: std::mem::size_of::<GpuPbrMaterial>() as u32,
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT as u32,
    })
}
