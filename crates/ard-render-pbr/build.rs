use std::{collections::HashMap, env, ffi::OsStr, io::BufWriter, path::PathBuf};

use ard_formats::vertex::VertexLayout;
use ard_pal::prelude::ShaderStage;
use ard_render_base::{shader_variant::ShaderVariant, RenderingMode};
use ard_render_material::factory::PassId;
use ard_render_renderers::passes::{
    COLOR_ALPHA_CUTOFF_PASS_ID, COLOR_OPAQUE_PASS_ID, DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
    DEPTH_OPAQUE_PREPASS_PASS_ID, HIGH_Z_PASS_ID, PATH_TRACER_PASS_ID, SHADOW_ALPHA_CUTOFF_PASS_ID,
    SHADOW_OPAQUE_PASS_ID, TRANSPARENT_PASS_ID,
};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    // List of shader variants
    // NOTE: The shader stages used here have a special meaning.
    // A shader with "AllGraphics" has a mesh, task, and fragment shader.
    // A shader with "Vertex" only has mesh and task shaders.
    let variants = [
        // High Z passes
        ShaderVariant {
            pass: usize::from(HIGH_Z_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
            rendering_mode: RenderingMode::Opaque,
        },
        // Shadow passes
        ShaderVariant {
            pass: usize::from(SHADOW_OPAQUE_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(SHADOW_ALPHA_CUTOFF_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::Vertex,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(SHADOW_ALPHA_CUTOFF_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        // Depth prepasses
        ShaderVariant {
            pass: usize::from(DEPTH_OPAQUE_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(DEPTH_OPAQUE_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(DEPTH_OPAQUE_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID),
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        // Color passes
        ShaderVariant {
            pass: usize::from(COLOR_OPAQUE_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(COLOR_OPAQUE_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(COLOR_OPAQUE_PASS_ID),
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(COLOR_ALPHA_CUTOFF_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(COLOR_ALPHA_CUTOFF_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(COLOR_ALPHA_CUTOFF_PASS_ID),
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        // Transparent passes
        ShaderVariant {
            pass: usize::from(TRANSPARENT_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Transparent,
        },
        ShaderVariant {
            pass: usize::from(TRANSPARENT_PASS_ID),
            vertex_layout: VertexLayout::UV0,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Transparent,
        },
        ShaderVariant {
            pass: usize::from(TRANSPARENT_PASS_ID),
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
            stage: ShaderStage::AllGraphics,
            rendering_mode: RenderingMode::Transparent,
        },
        // Path tracing
        ShaderVariant {
            pass: usize::from(PATH_TRACER_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::RayClosestHit,
            rendering_mode: RenderingMode::Opaque,
        },
        ShaderVariant {
            pass: usize::from(PATH_TRACER_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::RayClosestHit,
            rendering_mode: RenderingMode::AlphaCutout,
        },
        ShaderVariant {
            pass: usize::from(PATH_TRACER_PASS_ID),
            vertex_layout: VertexLayout::empty(),
            stage: ShaderStage::RayClosestHit,
            rendering_mode: RenderingMode::Transparent,
        },
    ];

    let mut out = HashMap::default();
    for variant in variants {
        compile_shader_variant(&out_dir, variant, &mut out);
    }

    let out_dir = PathBuf::from(&out_dir).join("pbr_variants.bin");

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(out_dir)
        .unwrap();
    let writer = BufWriter::new(file);
    bincode::serialize_into(writer, &out).unwrap();
}

fn compile_shader_variant(
    out_dir: &OsStr,
    variant: ShaderVariant,
    out: &mut HashMap<ShaderVariant, Vec<u8>>,
) {
    let mut defines = Vec::default();

    defines.push(
        match PassId::new(variant.pass) {
            HIGH_Z_PASS_ID => "HIGH_Z_PASS",
            SHADOW_OPAQUE_PASS_ID | SHADOW_ALPHA_CUTOFF_PASS_ID => "SHADOW_PASS",
            DEPTH_OPAQUE_PREPASS_PASS_ID | DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID => "DEPTH_PREPASS",
            COLOR_OPAQUE_PASS_ID | COLOR_ALPHA_CUTOFF_PASS_ID => "COLOR_PASS",
            TRANSPARENT_PASS_ID => "TRANSPARENT_PASS",
            PATH_TRACER_PASS_ID => "PATH_TRACE_PASS",
            _ => unreachable!("must implement for all passes"),
        }
        .into(),
    );

    match PassId::new(variant.pass) {
        DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID
        | COLOR_ALPHA_CUTOFF_PASS_ID
        | SHADOW_ALPHA_CUTOFF_PASS_ID => {
            defines.push("ALPHA_CUTOFF_PASS".into());
        }
        _ => {}
    }

    defines.push(format!(
        "ARD_VS_HAS_TANGENT={}",
        variant.vertex_layout.contains(VertexLayout::TANGENT) as u32
    ));
    defines.push(format!(
        "ARD_VS_HAS_UV0={}",
        variant.vertex_layout.contains(VertexLayout::UV0) as u32
    ));
    defines.push(format!(
        "ARD_VS_HAS_UV1={}",
        variant.vertex_layout.contains(VertexLayout::UV1) as u32
    ));

    match PassId::new(variant.pass) {
        PATH_TRACER_PASS_ID => {
            let out_dir = PathBuf::from(&out_dir).join("pbr.rchit.spv");
            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.rchit",
                &out_dir,
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );

            let bin = std::fs::read(out_dir).unwrap();
            out.insert(variant, bin);
        }
        _ => {
            let ts_name = PathBuf::from(&out_dir).join("pbr.ts.spv");
            let ms_name = PathBuf::from(&out_dir).join("pbr.ms.spv");
            let fs_name = PathBuf::from(&out_dir).join("pbr.fs.spv");

            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.ts.glsl",
                &ts_name,
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );

            let bin = std::fs::read(ts_name).unwrap();
            out.insert(
                ShaderVariant {
                    pass: variant.pass,
                    vertex_layout: variant.vertex_layout,
                    stage: ShaderStage::Task,
                    rendering_mode: variant.rendering_mode,
                },
                bin,
            );

            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.ms.glsl",
                &ms_name,
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );

            let bin = std::fs::read(ms_name).unwrap();
            out.insert(
                ShaderVariant {
                    pass: variant.pass,
                    vertex_layout: variant.vertex_layout,
                    stage: ShaderStage::Mesh,
                    rendering_mode: variant.rendering_mode,
                },
                bin,
            );

            if variant.stage == ShaderStage::AllGraphics {
                ard_render_codegen::vulkan_spirv::compile_shader(
                    "./shaders/pbr.frag",
                    &fs_name,
                    &["./shaders/", "../ard-render/shaders/"],
                    &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );

                let bin = std::fs::read(fs_name).unwrap();
                out.insert(
                    ShaderVariant {
                        pass: variant.pass,
                        vertex_layout: variant.vertex_layout,
                        stage: ShaderStage::Fragment,
                        rendering_mode: variant.rendering_mode,
                    },
                    bin,
                );
            }
        }
    }
}
