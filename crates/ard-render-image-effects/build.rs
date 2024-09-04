use std::{env, path::PathBuf};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=./shaders/");
    println!("cargo:rurun-if-changed={}", ard_render_si::GLSL_INCLUDE_DIR);

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/smaa/reset_edges.comp",
        PathBuf::from(&out_dir).join("smaa_reset_edges.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/smaa/edges.comp",
        PathBuf::from(&out_dir).join("smaa_edges.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/smaa/weights.comp",
        PathBuf::from(&out_dir).join("smaa_weights.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/smaa/blend.vert",
        PathBuf::from(&out_dir).join("smaa_blend.vert.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/smaa/blend.frag",
        PathBuf::from(&out_dir).join("smaa_blend.frag.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao/depth_prefilter.comp",
        PathBuf::from(&out_dir).join("ao_depth_prefilter.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao/main_pass.comp",
        PathBuf::from(&out_dir).join("ao_main_pass.comp.spv"),
        &["./shaders/", "../ard-render/shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao/denoise.comp",
        PathBuf::from(&out_dir).join("ao_denoise_pass.comp.spv"),
        &["./shaders/"],
        &[],
    );

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/ao/bilateral_filter.comp",
        PathBuf::from(&out_dir).join("ao_bilateral_filter.comp.spv"),
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

    ard_render_codegen::vulkan_spirv::compile_shader(
        "./shaders/lxaa.frag",
        PathBuf::from(&out_dir).join("lxaa.frag.spv"),
        &["./shaders/"],
        &[],
    );
}
