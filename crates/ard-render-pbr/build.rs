use std::{env, path::PathBuf};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/pbr.vert",
        PathBuf::from(&out_dir).join("pbr.vert.spv"),
        &["./shaders/"],
        &[],
    );
    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/pbr.frag",
        PathBuf::from(&out_dir).join("pbr.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
