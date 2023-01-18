use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    compile(
        Path::new("./examples/shaders/pbr.frag"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/pbr.depth.frag"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/pbr.vert"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/slice_vis.vert"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/slice_vis.frag"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cluster_heatmap.vert"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cluster_heatmap.frag"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cascade_vis.vert"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cascade_vis.frag"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cube.vert"),
        Path::new("./examples/assets/new_render/"),
    );
    compile(
        Path::new("./examples/shaders/cube.frag"),
        Path::new("./examples/assets/new_render/"),
    );

    compile(
        Path::new("./data/assets/shaders/pbr.frag"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/pbr.depth.frag"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/pbr.vert"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/slice_vis.vert"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/slice_vis.frag"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/cluster_heatmap.vert"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/cluster_heatmap.frag"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/cascade_vis.vert"),
        Path::new("./data/assets/standard/shaders/"),
    );
    compile(
        Path::new("./data/assets/shaders/cascade_vis.frag"),
        Path::new("./data/assets/standard/shaders/"),
    );
}

fn compile(in_path: &Path, out_path: &Path) {
    // Construct the output name of the shader
    let mut out_name: PathBuf = out_path.into();
    out_name.push(in_path.file_name().unwrap());

    let mut extension: String = in_path.extension().unwrap().to_str().unwrap().into();
    extension.push_str(".spv");

    out_name.set_extension(&extension);

    // Compile the shader
    let err = format!("unable to compile {:?}", out_name);
    let stderr = Command::new("glslc")
        .arg(in_path)
        .arg("-g")
        .arg("-I./crates/ard-render/src/shaders/include")
        .arg("--target-env=vulkan1.2")
        .arg("--target-spv=spv1.4")
        .arg("-o")
        .arg(&out_name)
        .output()
        .expect(&err)
        .stderr;

    if !stderr.is_empty() {
        let err = String::from_utf8(stderr).unwrap();
        panic!("unable to compile {:?}:\n{}", in_path, err);
    }
}
