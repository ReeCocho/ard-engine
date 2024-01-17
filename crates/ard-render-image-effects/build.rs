use std::{env, path::PathBuf};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao_construct.comp",
        PathBuf::from(&out_dir).join("ao_construct.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao_blur.comp",
        PathBuf::from(&out_dir).join("ao_blur.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/adaptive_lum_histogram_gen.comp",
        PathBuf::from(&out_dir).join("adaptive_lum_histogram_gen.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/adaptive_lum.comp",
        PathBuf::from(&out_dir).join("adaptive_lum.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/graphics_effect.vert",
        PathBuf::from(&out_dir).join("graphics_effect.vert.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/tonemapping.frag",
        PathBuf::from(&out_dir).join("tonemapping.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
