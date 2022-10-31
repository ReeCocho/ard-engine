use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    compile(Path::new("./src/er_to_cube.frag"), Path::new("./src/"));
    compile(Path::new("./src/er_to_cube.vert"), Path::new("./src/"));
    compile(
        Path::new("./src/diffuse_irradiance.frag"),
        Path::new("./src/"),
    );
    compile(
        Path::new("./src/prefiltered_env_map.frag"),
        Path::new("./src/"),
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
        .arg("-O")
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
