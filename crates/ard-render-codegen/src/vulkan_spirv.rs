use std::{ffi::OsStr, path::PathBuf, process::Command};

pub fn compile_shader(
    shader_path: impl Into<PathBuf> + AsRef<OsStr>,
    output_path: impl Into<PathBuf> + AsRef<OsStr>,
    include_paths: &[impl Into<PathBuf> + AsRef<OsStr>],
    defines: &[&str],
) {
    let shader_path = shader_path.into();
    let output_path: PathBuf = output_path.into();

    // Construct include paths
    let mut inc_paths = Vec::with_capacity(include_paths.len() + 1);
    inc_paths.push(format!("-I{}", ard_render_si::GLSL_INCLUDE_DIR));
    for path in include_paths {
        let path: PathBuf = path.into();
        inc_paths.push(format!("-I{}", path.to_str().unwrap()));
    }

    // Construct define arguments
    let mut def_args = Vec::with_capacity(defines.len());
    for def in defines {
        def_args.push(format!("-D{def}"));
    }

    // Create path if it doesn't exist yet
    let mut path_to_out = output_path.clone();
    path_to_out.pop();
    if let Err(err) = std::fs::create_dir_all(&path_to_out) {
        println!("cargo:warning=Unable to create directory `{path_to_out:?}`. Error: {err:?}");
    }

    // Compile the shader
    let stderr = match Command::new("glslc")
        .arg(&shader_path)
        .args(&inc_paths)
        .args(&def_args)
        .arg("--target-env=vulkan1.3")
        .arg("--target-spv=spv1.6")
        .arg("-g") // Enable for shader source debugging
        .arg("-o")
        .arg(output_path)
        .output()
    {
        Ok(res) => res.stderr,
        Err(err) => {
            println!("cargo:warning=Unable to compile `{shader_path:?}`. Error: {err:?}");
            return;
        }
    };

    if !stderr.is_empty() {
        let err = String::from_utf8(stderr).unwrap();
        println!("cargo:warning=Unable to compile `{shader_path:?}`. Error: {err:?}");
    }
}
