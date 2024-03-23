use std::{env, path::PathBuf};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/hzb_gen.comp",
        PathBuf::from(&out_dir).join("hzb_gen.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/gui.vert",
        PathBuf::from(&out_dir).join("gui.vert.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/gui.frag",
        PathBuf::from(&out_dir).join("gui.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
