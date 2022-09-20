use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    compile(
        Path::new("./examples/shaders/triangle.vert"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/triangle.frag"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/cube.vert"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/cube.frag"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/uniform_buffer.vert"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/vertex_compute.comp"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/index_compute.comp"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/test1_pal.comp"),
        Path::new("./examples/shaders/"),
    );
    compile(
        Path::new("./examples/shaders/test1_wgpu.comp"),
        Path::new("./examples/shaders/"),
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
