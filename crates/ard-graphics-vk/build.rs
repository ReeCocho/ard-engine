use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    // Compile shaders
    compile(
        Path::new("./src/renderer/draw_gen.comp"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/point_light_gen.comp"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/quad.vert"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/zreduce.frag"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/debug_draw.vert"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/debug_draw.frag"),
        Path::new("./src/renderer/"),
    );
    compile(
        Path::new("./src/renderer/cluster_gen.comp"),
        Path::new("./src/renderer/"),
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
        .arg("--target-env=vulkan1.1")
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
