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
        "./shaders/sun_shafts.comp",
        PathBuf::from(&out_dir).join("sun_shafts.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/sun_shafts/shafts_gen_lines.comp",
        PathBuf::from(&out_dir).join("shafts_gen_lines.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/sun_shafts/shafts_interpolate.comp",
        PathBuf::from(&out_dir).join("shafts_interpolate.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/sun_shafts/shafts_refine.comp",
        PathBuf::from(&out_dir).join("shafts_refine.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/sun_shafts/shafts_sample.comp",
        PathBuf::from(&out_dir).join("shafts_sample.comp.spv"),
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

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/fxaa.frag",
        PathBuf::from(&out_dir).join("fxaa.frag.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/bloom_downscale.frag",
        PathBuf::from(&out_dir).join("bloom_downscale.frag.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/bloom_upscale.frag",
        PathBuf::from(&out_dir).join("bloom_upscale.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
