use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    // Compile shaders
    // compile(
    //     Path::new("./examples/shaders/color.frag"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/pbr.frag"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/pbr.vert"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/textured.frag"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/textured.vert"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/triangle.frag"),
    //     Path::new("./examples/assets/example/"),
    // );
    // compile(
    //     Path::new("./examples/shaders/triangle.vert"),
    //     Path::new("./examples/assets/example/"),
    // );
    compile(
        Path::new("./examples/shaders/new_rend.frag"),
        Path::new("./examples/assets/example/"),
    );
    compile(
        Path::new("./examples/shaders/new_rend.vert"),
        Path::new("./examples/assets/example/"),
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
        .arg("--target-env=vulkan1.2")
        .arg("-I./crates/ard-render/src/shaders/include")
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
