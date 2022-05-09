use ard_engine::{
    core::prelude::*,
    ecs::prelude::*,
    graphics::prelude::*,
    math::{Mat4, Vec3, Vec4},
    window::prelude::*,
};

fn main() {
    AppBuilder::new()
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("Triangle"),
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(VkGraphicsPlugin {
            context_create_info: GraphicsContextCreateInfo {
                window: WindowId::primary(),
                debug: true,
            },
        })
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Shaders
    let create_info = ShaderCreateInfo {
        ty: ShaderType::Fragment,
        code: include_bytes!("./triangle.frag.spv"),
        vertex_layout: VertexLayout::default(),
        inputs: ShaderInputs {
            ubo_size: 0,
            texture_count: 0,
        },
    };

    let frag_shader = factory.create_shader(&create_info);

    let create_info = ShaderCreateInfo {
        ty: ShaderType::Vertex,
        code: include_bytes!("./triangle.vert.spv"),
        vertex_layout: VertexLayout {
            colors: true,
            ..Default::default()
        },
        inputs: ShaderInputs {
            ubo_size: 0,
            texture_count: 0,
        },
    };

    let vert_shader = factory.create_shader(&create_info);

    // Pipeline
    let create_info = PipelineCreateInfo {
        vertex: vert_shader.clone(),
        fragment: frag_shader.clone(),
    };

    let pipeline = factory.create_pipeline(&create_info);

    // Material
    let create_info = MaterialCreateInfo { pipeline };

    let material = factory.create_material(&create_info);

    // Mesh
    let positions = [
        Vec4::new(-1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
    ];

    let colors = [
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
    ];

    let indices = [0, 1, 2];

    let create_info = MeshCreateInfo {
        positions: &positions,
        colors: Some(&colors),
        indices: &indices,
        ..Default::default()
    };

    let mesh = factory.create_mesh(&create_info);

    // Register draws
    // NOTE: Unless you want the draw to exist forever, you should store the handles generated here
    // so they can be unregistered later
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();
    draws.register(
        &[(
            Renderable { mesh, material },
            Model(Mat4::from_translation(Vec3::new(0.0, -0.5, 1.0))),
        )],
        &mut [],
    );
}
