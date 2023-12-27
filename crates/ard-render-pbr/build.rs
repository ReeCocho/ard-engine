use std::{env, ffi::OsStr, path::PathBuf};

struct ShaderVariant<'a> {
    pub ext: &'static str,
    pub defines: &'a [&'static str],
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    // List of shader variants
    let variants = [
        ShaderVariant {
            ext: "",
            defines: &[
                "ARD_VS_HAS_TANGENT=0",
                "ARD_VS_HAS_COLOR=0",
                "ARD_VS_HAS_UV0=0",
                "ARD_VS_HAS_UV1=0",
                "ARD_VS_HAS_UV2=0",
                "ARD_VS_HAS_UV3=0",
            ],
        },
        ShaderVariant {
            ext: ".tuv0",
            defines: &[
                "ARD_VS_HAS_TANGENT=1",
                "ARD_VS_HAS_COLOR=0",
                "ARD_VS_HAS_UV0=1",
                "ARD_VS_HAS_UV1=0",
                "ARD_VS_HAS_UV2=0",
                "ARD_VS_HAS_UV3=0",
            ],
        },
        ShaderVariant {
            ext: ".uv0d",
            defines: &[
                "DEPTH_ONLY",
                "ARD_VS_HAS_TANGENT=0",
                "ARD_VS_HAS_COLOR=0",
                "ARD_VS_HAS_UV0=1",
                "ARD_VS_HAS_UV1=0",
                "ARD_VS_HAS_UV2=0",
                "ARD_VS_HAS_UV3=0",
            ],
        },
    ];

    for variant in variants {
        compile_shader_variant(&out_dir, variant);
    }
}

fn compile_shader_variant(out_dir: &OsStr, variant: ShaderVariant) {
    let vert_name = format!("pbr.vert{}.spv", variant.ext);
    let frag_name = format!("pbr.frag{}.spv", variant.ext);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/pbr.vert",
        PathBuf::from(&out_dir).join(vert_name),
        &["./shaders/"],
        variant.defines,
    );
    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/pbr.frag",
        PathBuf::from(&out_dir).join(frag_name),
        &["./shaders/"],
        variant.defines,
    );
}
