use std::process::Command;

fn main() {
    // Compile shaders
    compile("draw_gen.comp");
    compile("point_light_gen.comp");
    compile("quad.vert");
    compile("zreduce.frag");
    compile("debug_draw.vert");
    compile("debug_draw.frag");
    compile("cluster_gen.comp");
}

fn compile(name: &str) {
    let mut out_name = String::from(name);
    out_name.push_str(".spv");

    let err = format!("unable to compile {}", name);

    let stderr = Command::new("glslc")
        .current_dir("./src/renderer/")
        .arg(name)
        .arg("--target-env=vulkan1.1")
        .arg("-o")
        .arg(&out_name)
        .output()
        .expect(&err)
        .stderr;

    if !stderr.is_empty() {
        let err = String::from_utf8(stderr).unwrap();
        panic!("unable to compile {}:\n{}", name, err);
    }
}
