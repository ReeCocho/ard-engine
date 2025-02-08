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
        "./shaders/di_gather.comp",
        PathBuf::from(&out_dir).join("di_gather.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/di_par_reduce.comp",
        PathBuf::from(&out_dir).join("di_par_reduce.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/di_render.frag",
        PathBuf::from(&out_dir).join("di_render.frag.spv"),
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
        "./shaders/proc_skybox.vert",
        PathBuf::from(&out_dir).join("proc_skybox.color_pass.vert.spv"),
        &["./shaders/"],
        &["COLOR_PASS"],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/proc_skybox.frag",
        PathBuf::from(&out_dir).join("proc_skybox.frag.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/proc_skybox.frag",
        PathBuf::from(&out_dir).join("proc_skybox.color_pass.frag.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &["COLOR_PASS"],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/env_prefilter.frag",
        PathBuf::from(&out_dir).join("env_prefilter.frag.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflection_reset.comp",
        PathBuf::from(&out_dir).join("reflection_reset.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/tile_classifier.comp",
        PathBuf::from(&out_dir).join("tile_classifier.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/raygen.comp",
        PathBuf::from(&out_dir).join("raygen.comp.spv"),
        &[
            "./shaders/",
            "../ard-render/shaders/",
            "../ard-render-pbr/shaders/",
        ],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflections.rgen",
        PathBuf::from(&out_dir).join("reflections.rgen.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflections.rmiss",
        PathBuf::from(&out_dir).join("reflections.rmiss.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflection_accum.comp",
        PathBuf::from(&out_dir).join("reflection_accum.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflection_apply.vert",
        PathBuf::from(&out_dir).join("reflection_apply.vert.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/reflections/reflection_apply.frag",
        PathBuf::from(&out_dir).join("reflection_apply.frag.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );
}
