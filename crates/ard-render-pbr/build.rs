use std::{env, ffi::OsStr, path::PathBuf};

use ard_formats::vertex::VertexLayout;

struct ShaderVariant {
    pub vertex_layout: VertexLayout,
    pub pass: Pass,
}

#[derive(Copy, Clone)]
enum Pass {
    HighZ,
    Shadow,
    ShadowAc,
    DepthPrepass,
    DepthPrepassAc,
    Color,
    ColorAc,
    Transparent,
    RayTrace,
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    // List of shader variants
    let variants = [
        // High Z passes
        ShaderVariant {
            pass: Pass::HighZ,
            vertex_layout: VertexLayout::empty(),
        },
        // Shadow passes
        ShaderVariant {
            pass: Pass::Shadow,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::ShadowAc,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::ShadowAc,
            vertex_layout: VertexLayout::UV0,
        },
        // Depth prepasses
        ShaderVariant {
            pass: Pass::DepthPrepass,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::DepthPrepass,
            vertex_layout: VertexLayout::UV0,
        },
        ShaderVariant {
            pass: Pass::DepthPrepass,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
        },
        ShaderVariant {
            pass: Pass::DepthPrepassAc,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::DepthPrepassAc,
            vertex_layout: VertexLayout::UV0,
        },
        ShaderVariant {
            pass: Pass::DepthPrepassAc,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
        },
        // Color passes
        ShaderVariant {
            pass: Pass::Color,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::Color,
            vertex_layout: VertexLayout::UV0,
        },
        ShaderVariant {
            pass: Pass::Color,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
        },
        ShaderVariant {
            pass: Pass::ColorAc,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::ColorAc,
            vertex_layout: VertexLayout::UV0,
        },
        ShaderVariant {
            pass: Pass::ColorAc,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
        },
        // Transparent passes
        ShaderVariant {
            pass: Pass::Transparent,
            vertex_layout: VertexLayout::empty(),
        },
        ShaderVariant {
            pass: Pass::Transparent,
            vertex_layout: VertexLayout::UV0,
        },
        ShaderVariant {
            pass: Pass::Transparent,
            vertex_layout: VertexLayout::UV0 | VertexLayout::TANGENT,
        },
        // Ray tracing
        ShaderVariant {
            pass: Pass::RayTrace,
            vertex_layout: VertexLayout::empty(),
        },
    ];

    for variant in variants {
        compile_shader_variant(&out_dir, variant);
    }
}

fn compile_shader_variant(out_dir: &OsStr, variant: ShaderVariant) {
    let mut ext = format!(".{}", variant.pass);

    if !variant.vertex_layout.is_empty() {
        let mut vl = String::from(".");

        if variant.vertex_layout.contains(VertexLayout::TANGENT) {
            vl.push_str("t");
        }

        if variant.vertex_layout.contains(VertexLayout::UV0) {
            vl.push_str("uv0");
        }

        if variant.vertex_layout.contains(VertexLayout::UV1) {
            vl.push_str("uv1");
        }

        ext.push_str(&vl);
    }

    let mut defines = Vec::default();

    defines.push(
        match variant.pass {
            Pass::HighZ => "HIGH_Z_PASS",
            Pass::Shadow | Pass::ShadowAc => "SHADOW_PASS",
            Pass::DepthPrepass | Pass::DepthPrepassAc => "DEPTH_PREPASS",
            Pass::Color | Pass::ColorAc => "COLOR_PASS",
            Pass::Transparent => "TRANSPARENT_PASS",
            Pass::RayTrace => "RT_PASS",
        }
        .into(),
    );

    match variant.pass {
        Pass::DepthPrepassAc | Pass::ColorAc | Pass::ShadowAc => {
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

    match variant.pass {
        Pass::RayTrace => {
            let rchit_name = format!("pbr.rchit{}.spv", ext);

            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.rchit",
                PathBuf::from(&out_dir).join(rchit_name),
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
        }
        _ => {
            let ts_name = format!("pbr.task{}.spv", ext);
            let ms_name = format!("pbr.mesh{}.spv", ext);
            let frag_name = format!("pbr.frag{}.spv", ext);

            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.ms.glsl",
                PathBuf::from(&out_dir).join(ms_name),
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.ts.glsl",
                PathBuf::from(&out_dir).join(ts_name),
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            ard_render_codegen::vulkan_spirv::compile_shader(
                "./shaders/pbr.frag",
                PathBuf::from(&out_dir).join(frag_name),
                &["./shaders/", "../ard-render/shaders/"],
                &defines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
        }
    }
}

impl std::fmt::Display for Pass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pass::HighZ => write!(f, "high_z"),
            Pass::Shadow => write!(f, "shadow"),
            Pass::ShadowAc => write!(f, "shadow_ac"),
            Pass::DepthPrepass => write!(f, "depth_prepass"),
            Pass::DepthPrepassAc => write!(f, "depth_prepass_ac"),
            Pass::Color => write!(f, "color"),
            Pass::ColorAc => write!(f, "color_ac"),
            Pass::Transparent => write!(f, "transparent"),
            Pass::RayTrace => write!(f, "rt"),
        }
    }
}
