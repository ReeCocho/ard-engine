use std::collections::{BTreeMap, HashMap};

use ard_formats::vertex::VertexLayout;
use ard_pal::prelude::{
    BlendFactor, BlendOp, ColorBlendAttachment, ColorBlendState, ColorComponents, CompareOp,
    CullMode, DepthStencilState, FrontFace, GraphicsProperties, PolygonMode, RasterizationState,
    ShaderStage,
};
use ard_render_base::{shader_variant::ShaderVariant, RenderingMode};
use ard_render_material::{
    factory::PassId,
    material::{Material, MaterialCreateInfo, MaterialVariantDescriptor, RtVariantDescriptor},
    material_instance::TextureSlot,
    shader::{Shader, ShaderCreateInfo},
};
use ard_render_renderers::passes::{
    COLOR_ALPHA_CUTOFF_PASS_ID, COLOR_OPAQUE_PASS_ID, DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
    DEPTH_OPAQUE_PREPASS_PASS_ID, HIGH_Z_PASS_ID, PATH_TRACER_PASS_ID, SHADOW_ALPHA_CUTOFF_PASS_ID,
    SHADOW_OPAQUE_PASS_ID, TRANSPARENT_PASS_ID,
};
use ard_render_si::{consts::*, types::GpuPbrMaterial};

pub type PbrMaterialData = GpuPbrMaterial;

pub const PBR_MATERIAL_TEXTURE_COUNT: usize = 3;
pub const PBR_MATERIAL_DIFFUSE_SLOT: TextureSlot = TextureSlot(0);
pub const PBR_MATERIAL_NORMAL_SLOT: TextureSlot = TextureSlot(1);
pub const PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT: TextureSlot = TextureSlot(2);

/// Creates the PBR material given functions that can create shader modules and materials (this is
/// probably going to be a wrapper for the factories shader creation function).
pub fn create_pbr_material(
    properties: &GraphicsProperties,
    create_shader: impl Fn(ShaderCreateInfo) -> Shader,
    create_material: impl Fn(MaterialCreateInfo) -> Material,
) -> Material {
    #[derive(Clone)]
    pub struct MaterialVariantTemplate {
        pub rasterization: RasterizationState,
        pub depth_stencil: Option<DepthStencilState>,
        pub color_blend: ColorBlendState,
        pub debug_name: String,
    }

    const SHADER_VARIANTS: &'static [u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "./pbr_variants.bin"));
    let variant_code =
        bincode::deserialize::<HashMap<ShaderVariant, Vec<u8>>>(SHADER_VARIANTS).unwrap();

    // Compile variants
    let mut variant_shaders = HashMap::<ShaderVariant, Shader>::default();
    for (variant, code) in variant_code.iter() {
        let task_invocs = match PassId::new(variant.pass) {
            // Shadow, pre-depth, and transparent passes have to have more than one task
            // invocations to accelerate culling.
            SHADOW_OPAQUE_PASS_ID
            | SHADOW_ALPHA_CUTOFF_PASS_ID
            | DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID
            | DEPTH_OPAQUE_PREPASS_PASS_ID
            | TRANSPARENT_PASS_ID => invocations_per_task(properties),
            _ => 1,
        };

        variant_shaders.insert(
            *variant,
            create_shader(ShaderCreateInfo {
                code,
                debug_name: Some(match variant.stage {
                    ShaderStage::Task => "pbr_task".into(),
                    ShaderStage::Mesh => "pbr_mesh".into(),
                    ShaderStage::Fragment => "pbr_frag".into(),
                    ShaderStage::RayClosestHit => "pbr_rchit".into(),
                    _ => String::default(),
                }),
                texture_slots: PBR_MATERIAL_TEXTURE_COUNT,
                data_size: std::mem::size_of::<GpuPbrMaterial>(),
                work_group_size: match variant.stage {
                    ShaderStage::Task => (task_invocs, 1, 1),
                    ShaderStage::Mesh => (invocations_per_mesh(properties), 1, 1),
                    _ => (0, 0, 0),
                },
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
                depth_compare: CompareOp::GreaterOrEqual,
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
                depth_compare: CompareOp::GreaterOrEqual,
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
                    write_mask: ColorComponents::R | ColorComponents::G | ColorComponents::B,
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
                    write_mask: ColorComponents::R | ColorComponents::G | ColorComponents::B,
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
    let mut rt_variants = BTreeMap::<PassId, Vec<RtVariantDescriptor>>::default();

    for (variant, ray_shader) in variant_shaders.iter() {
        // If this is a ray tracing shader, create the variant
        if variant.stage == ShaderStage::RayClosestHit || variant.stage == ShaderStage::RayAnyHit {
            let entry = rt_variants.entry(PassId::new(variant.pass)).or_default();
            entry.push(RtVariantDescriptor {
                vertex_layout: VertexLayout::POSITION
                    | VertexLayout::NORMAL
                    | variant.vertex_layout,
                rendering_mode: variant.rendering_mode,
                shader: variant_shaders
                    .get(&ShaderVariant {
                        pass: variant.pass,
                        vertex_layout: variant.vertex_layout,
                        stage: variant.stage,
                        rendering_mode: variant.rendering_mode,
                    })
                    .unwrap()
                    .clone(),
                stage: variant.stage,
            });
            continue;
        }

        // Skip if this is not a task shader
        if variant.stage != ShaderStage::Task {
            continue;
        }

        let tshader = ray_shader;

        // Get associated mesh shader
        let mshader = variant_shaders
            .get(&ShaderVariant {
                pass: variant.pass,
                vertex_layout: variant.vertex_layout,
                stage: ShaderStage::Mesh,
                rendering_mode: variant.rendering_mode,
            })
            .unwrap();

        // Get the associated fragment shader (if available)
        let fshader = variant_shaders
            .get(&ShaderVariant {
                pass: variant.pass,
                vertex_layout: variant.vertex_layout,
                stage: ShaderStage::Fragment,
                rendering_mode: variant.rendering_mode,
            })
            .map(|s| s.clone());

        // Get template
        let template = pass_templates.get(&PassId::new(variant.pass)).unwrap();

        variants.push(MaterialVariantDescriptor {
            pass_id: PassId::new(variant.pass),
            vertex_layout: VertexLayout::POSITION | VertexLayout::NORMAL | variant.vertex_layout,
            task_shader: tshader.clone(),
            mesh_shader: mshader.clone(),
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
        rt_variants,
        data_size: std::mem::size_of::<GpuPbrMaterial>() as u32,
        texture_slots: PBR_MATERIAL_TEXTURE_COUNT as u32,
    })
}

#[inline(always)]
pub fn invocations_per_task(props: &GraphicsProperties) -> u32 {
    props
        .mesh_shading
        .preferred_task_work_group_invocations
        .min(MAX_TASK_SHADER_INVOCATIONS)
}

#[inline(always)]
pub fn invocations_per_mesh(props: &GraphicsProperties) -> u32 {
    props
        .mesh_shading
        .preferred_mesh_work_group_invocations
        .min(MAX_PRIMITIVES)
}
