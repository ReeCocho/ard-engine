use std::{env, path::PathBuf};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/light_clustering.comp",
        PathBuf::from(&out_dir).join("light_clustering.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/proc_skybox.vert",
        PathBuf::from(&out_dir).join("proc_skybox.vert.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/proc_skybox.frag",
        PathBuf::from(&out_dir).join("proc_skybox.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
