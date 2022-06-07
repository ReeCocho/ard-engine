use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    path: String,
    #[clap(short, long, default_value = "./out")]
    out: String,
    #[clap(short, long, default_value_t = 5)]
    mip_count: usize,
    #[clap(short, long, default_value_t = 512)]
    out_width: u32,
    #[clap(short, long, default_value_t = 512)]
    out_height: u32,
}

const VERTEX_SHADER_SRC: &'static str = r#"
    #version 330 core

    const vec3 POINTS[] = vec3[36](
        // East
        vec3(1.0, -1.0,  1.0),
        vec3(1.0,  1.0,  1.0),
        vec3(1.0,  1.0, -1.0),
        
        vec3(1.0, -1.0,  1.0),
        vec3(1.0, -1.0, -1.0),
        vec3(1.0,  1.0, -1.0),
    
        // West
        vec3(-1.0, -1.0,  1.0),
        vec3(-1.0,  1.0,  1.0),
        vec3(-1.0,  1.0, -1.0),
        
        vec3(-1.0, -1.0,  1.0),
        vec3(-1.0, -1.0, -1.0),
        vec3(-1.0,  1.0, -1.0),
        // North
        vec3(-1.0, -1.0, 1.0),
        vec3(-1.0,  1.0, 1.0),
        vec3( 1.0,  1.0, 1.0),
    
        vec3(-1.0, -1.0, 1.0),
        vec3( 1.0,  1.0, 1.0),
        vec3( 1.0, -1.0, 1.0),
        
        // South
        vec3(-1.0, -1.0, -1.0),
        vec3(-1.0,  1.0, -1.0),
        vec3( 1.0,  1.0, -1.0),
    
        vec3(-1.0, -1.0, -1.0),
        vec3( 1.0,  1.0, -1.0),
        vec3( 1.0, -1.0, -1.0),
        
        // Top
        vec3( 1.0, 1.0,  1.0),
        vec3(-1.0, 1.0,  1.0),
        vec3(-1.0, 1.0, -1.0),
    
        vec3( 1.0, 1.0,  1.0),
        vec3(-1.0, 1.0, -1.0),
        vec3( 1.0, 1.0, -1.0),
    
        // Bottom
        vec3( 1.0, -1.0,  1.0),
        vec3(-1.0, -1.0,  1.0),
        vec3(-1.0, -1.0, -1.0),
    
        vec3( 1.0, -1.0,  1.0),
        vec3(-1.0, -1.0, -1.0),
        vec3( 1.0, -1.0, -1.0)
    );

    out vec3 LOCAL_POS;

    uniform mat4 vp;

    void main() {
        LOCAL_POS = POINTS[gl_VertexID];
        gl_Position = vp * vec4(LOCAL_POS, 1.0);
    }
"#;

/// This shader converts the equirectangular images we receive and renders them to the faces of
/// a cube map.
const FLAT_TO_CUBE_SHADER: &'static str = r#"
    #version 330 core

    out vec4 FRAG_COLOR;
    in vec3 LOCAL_POS;

    uniform sampler2D equirectangular_map;

    const vec2 inv_atan = vec2(0.1591, 0.3183);

    void main() {
        vec3 v = normalize(LOCAL_POS);
        
        vec2 uv = vec2(atan(v.z, v.x), asin(v.y));
        uv *= inv_atan;
        uv += 0.5;

        vec3 color = texture(equirectangular_map, uv).rgb;

        FRAG_COLOR = vec4(color, 1.0);
    }
"#;

fn main() {
    let args = Args::parse();

    println!("Building sky, irradiance, and radiance maps...");
    println!("Path to skybox : {}", &args.path);
    println!("Mip count for radiance map : {}", args.mip_count);
    println!("Output Directory : {}", &args.out);

    // Intitialize OpenGL
    println!("Initializing OpenGL...");

    let events_loop = glium::glutin::event_loop::EventLoop::new();

    let wb = glium::glutin::window::WindowBuilder::new()
        .with_title("ibl_baker")
        .with_visible(false);

    let cb = glium::glutin::ContextBuilder::new();

    let display = glium::Display::new(wb, cb, &events_loop).unwrap();

    // Create shader programs
    let flat_to_cube_program = glium::Program::from_source(&display, VERTEX_SHADER_SRC, FLAT_TO_CUBE_SHADER, None).unwrap();

    // Load in the image to process
    println!("Loading `{}`...", &args.path);
    let img = image::open(&args.path).unwrap().to_rgba8();
    let img_dimensions = img.dimensions();

    // Upload the image to an OpenGL texture.
    println!("Uploading to GPU...");
    let skybox = glium::texture::RawImage2d::from_raw_rgba_reversed(&img.into_raw(), img_dimensions);
    let skybox_texture = glium::texture::srgb_texture2d::SrgbTexture2d::new(&display, skybox).unwrap();

    // Convert the image into a cubemap
    glium::texture::CubeLayer::NegativeX;
    // let capture_fbo = glium::framebuffer::SimpleFrameBuffer::new(&display, color)

}
