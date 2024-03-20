use std::collections::HashMap;

use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{
    BlendFactor, BlendOp, ColorBlendAttachment, ColorBlendState, ColorComponents, CompareOp,
    CullMode, DepthStencilState, FrontFace, PolygonMode, RasterizationState, ShaderStage,
};
use ard_render_material::{
    factory::PassId,
    material::{Material, MaterialCreateInfo, MaterialVariantDescriptor},
    material_instance::TextureSlot,
    shader::{Shader, ShaderCreateInfo},
};
use ard_render_renderers::passes::{
    COLOR_ALPHA_CUTOFF_PASS_ID, COLOR_OPAQUE_PASS_ID, DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
    DEPTH_OPAQUE_PREPASS_PASS_ID, HIGH_Z_PASS_ID, SHADOW_ALPHA_CUTOFF_PASS_ID,
    SHADOW_OPAQUE_PASS_ID, TRANSPARENT_PASS_ID,
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
    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct ShaderVariant {
        pass: PassId,
        vertex_layout: VertexLayout,
        stage: ShaderStage,
    }

    #[derive(Clone)]
    pub struct MaterialVariantTemplate {
        pub rasterization: RasterizationState,
        pub depth_stencil: Option<DepthStencilState>,
        pub color_blend: ColorBlendState,
        pub debug_name: String,
    }

    let mut variant_code = HashMap::<ShaderVariant, &'static [u8]>::default();

    // High Z pass
    variant_code.insert(
        ShaderVariant {
            pass: HIGH_Z_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.high_z.spv")),
    );

    // Shadow pass
    variant_code.insert(
        ShaderVariant {
            pass: SHADOW_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.shadow.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: SHADOW_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.shadow_ac.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: SHADOW_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.shadow_ac.uv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: SHADOW_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.shadow_ac.uv0.spv")),
    );

    // Depth prepass
    variant_code.insert(
        ShaderVariant {
            pass: DEPTH_OPAQUE_PREPASS_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.depth_prepass.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.depth_prepass_ac.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(
            env!("OUT_DIR"),
            "./pbr.vert.depth_prepass_ac.uv0.spv"
        )),
    );
    variant_code.insert(
        ShaderVariant {
            pass: DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(
            env!("OUT_DIR"),
            "./pbr.frag.depth_prepass_ac.uv0.spv"
        )),
    );

    // Color pass
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color.uv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color.uv0.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color.tuv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_OPAQUE_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color.tuv0.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color_ac.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color_ac.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color_ac.uv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color_ac.uv0.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.color_ac.tuv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: COLOR_ALPHA_CUTOFF_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.color_ac.tuv0.spv")),
    );

    // Transparent pass
    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.transparent.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.transparent.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.transparent.uv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.transparent.uv0.spv")),
    );

    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Vertex,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.vert.transparent.tuv0.spv")),
    );
    variant_code.insert(
        ShaderVariant {
            pass: TRANSPARENT_PASS_ID,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::Fragment,
        },
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr.frag.transparent.tuv0.spv")),
    );

    // Compile variants
    let mut variant_shaders = HashMap::<ShaderVariant, Shader>::default();
    for (variant, code) in variant_code.iter() {
        variant_shaders.insert(
            *variant,
            create_shader(ShaderCreateInfo {
                code: *code,
                debug_name: Some(match variant.stage {
                    ShaderStage::Vertex => "pbr_vert".into(),
                    ShaderStage::Fragment => "pbr_frag".into(),
                    _ => String::default(),
                }),
                texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
                data_size: std::mem::size_of::<GpuPbrMaterial>(),
            }),
        );
    }

    // Templates for passes
    let mut pass_templates = HashMap::<PassId, MaterialVariantTemplate>::default();

    pass_templates.insert(
        HIGH_Z_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_hzb_pass_pipeline".into(),
        },
    );

    pass_templates.insert(
        SHADOW_OPAQUE_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_shadow_opaque_pass_pipeline".into(),
        },
    );

    pass_templates.insert(
        SHADOW_ALPHA_CUTOFF_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_shadow_alpha_cutoff_pass_pipeline".into(),
        },
    );

    pass_templates.insert(
        DEPTH_OPAQUE_PREPASS_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_depth_opaque_prepass_pipeline".into(),
        },
    );

    pass_templates.insert(
        DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_depth_alpha_cutoff_prepass_pipeline".into(),
        },
    );

    pass_templates.insert(
        COLOR_OPAQUE_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_color_opaque_pipeline".into(),
        },
    );

    pass_templates.insert(
        COLOR_ALPHA_CUTOFF_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_color_alpha_cutoff_pipeline".into(),
        },
    );

    pass_templates.insert(
        TRANSPARENT_PASS_ID,
        MaterialVariantTemplate {
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
            debug_name: "pbr_transparent_pipeline".into(),
        },
    );

    // Construct variants
    let mut variants = Vec::default();

    for (variant, vshader) in variant_shaders.iter() {
        // Skip if this is not a vertex shader
        if variant.stage != ShaderStage::Vertex {
            continue;
        }

        // Get the associated fragment shader (if available)
        let fshader = variant_shaders
            .get(&ShaderVariant {
                pass: variant.pass,
                vertex_layout: variant.vertex_layout,
                stage: ShaderStage::Fragment,
            })
            .map(|s| s.clone());

        // Get template
        let template = pass_templates.get(&variant.pass).unwrap();

        variants.push(MaterialVariantDescriptor {
            pass_id: variant.pass,
            vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL | variant.vertex_layout,
            vertex_shader: vshader.clone(),
            fragment_shader: fshader,
            rasterization: template.rasterization,
            depth_stencil: template.depth_stencil,
            color_blend: template.color_blend.clone(),
            debug_name: Some(template.debug_name.clone()),
        });
    }

    // Actual material creation info
    create_material(MaterialCreateInfo {
        variants,
        data_size: std::mem::size_of::<GpuPbrMaterial>() as u32,
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT as u32,
    })
}
