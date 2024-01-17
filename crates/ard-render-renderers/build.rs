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
        "./shaders/draw_gen.comp",
        PathBuf::from(&out_dir).join("draw_gen.comp.spv"),
        &["./shaders/"],
        &["HIGH_Z_CULLING"],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/draw_gen.comp",
        PathBuf::from(&out_dir).join("draw_gen_no_hzb.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/draw_compact.comp",
        PathBuf::from(&out_dir).join("draw_compact.comp.spv"),
        &["./shaders/"],
        &[],
    );
}
