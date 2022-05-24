use ard_engine::{core::prelude::*, graphics::prelude::*, math::*, window::prelude::*};

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Error)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("Materials"),
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
        ty: ShaderType::Vertex,
        code: include_bytes!("./triangle.vert.spv"),
        vertex_layout: VertexLayout {
            colors: true,
            ..Default::default()
        },
        inputs: ShaderInputs {
            ubo_size: std::mem::size_of::<Vec4>() as u64,
            texture_count: 0,
        },
    };

    let vert_shader = factory.create_shader(&create_info);

    let create_info = ShaderCreateInfo {
        ty: ShaderType::Fragment,
        code: include_bytes!("./color.frag.spv"),
        vertex_layout: VertexLayout::default(),
        inputs: ShaderInputs {
            ubo_size: std::mem::size_of::<Vec4>() as u64,
            texture_count: 0,
        },
    };

    let frag_shader = factory.create_shader(&create_info);

    // Pipeline
    let create_info = PipelineCreateInfo {
        vertex: vert_shader.clone(),
        fragment: frag_shader.clone(),
    };

    let pipeline = factory.create_pipeline(&create_info);

    // Meshes
    let positions = [
        Vec4::new(-0.5, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 0.5, 0.0, 1.0),
        Vec4::new(0.5, 0.0, 0.0, 1.0),
    ];

    let colors = [
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
    ];

    let indices = [0, 1, 2];

    let create_info = MeshCreateInfo {
        positions: &positions,
        colors: Some(&colors),
        indices: &indices,
        bounds: MeshBounds::Generate,
        ..Default::default()
    };

    let triangle_mesh = factory.create_mesh(&create_info);

    let positions = [
        Vec4::new(-0.5, -0.5, 0.0, 1.0),
        Vec4::new(-0.5, 0.5, 0.0, 1.0),
        Vec4::new(0.5, 0.5, 0.0, 1.0),
        Vec4::new(-0.5, -0.5, 0.0, 1.0),
        Vec4::new(0.5, 0.5, 0.0, 1.0),
        Vec4::new(0.5, -0.5, 0.0, 1.0),
    ];

    let colors = [
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
    ];

    let indices = [0, 1, 2, 3, 4, 5];

    let create_info = MeshCreateInfo {
        positions: &positions,
        colors: Some(&colors),
        indices: &indices,
        bounds: MeshBounds::Generate,
        ..Default::default()
    };

    let quad_mesh = factory.create_mesh(&create_info);

    // Update camera
    const WIDTH: usize = 10;
    const HEIGHT: usize = 10;

    let main_camera = factory.main_camera();

    let position = Vec3::new(0.0, HEIGHT as f32 / 2.0, -(WIDTH as f32));

    let center = Vec3::new(0.0, HEIGHT as f32 / 2.0, 0.0);

    let descriptor = CameraDescriptor {
        position,
        center,
        far: 500.0,
        ..Default::default()
    };

    factory.update_camera(&main_camera, descriptor);

    // Create draws
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();

    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            let color = Vec4::new(x as f32 / WIDTH as f32, y as f32 / HEIGHT as f32, 0.0, 1.0);

            // Register triangle
            let create_info = MaterialCreateInfo {
                pipeline: pipeline.clone(),
            };
            let material = factory.create_material(&create_info);
            factory.update_material_data(&material, bytemuck::cast_slice(&[color]));

            draws.register(
                &[(
                    Renderable {
                        mesh: triangle_mesh.clone(),
                        material: material.clone(),
                    },
                    Model(Mat4::from_translation(Vec3::new(
                        -(x as f32) - 1.0,
                        y as f32,
                        1.0,
                    ))),
                )],
                &mut [],
            );

            // Register quad
            let create_info = MaterialCreateInfo {
                pipeline: pipeline.clone(),
            };
            let material = factory.create_material(&create_info);
            factory.update_material_data(&material, bytemuck::cast_slice(&[color]));

            draws.register(
                &[(
                    Renderable {
                        mesh: quad_mesh.clone(),
                        material: material.clone(),
                    },
                    Model(Mat4::from_translation(Vec3::new(
                        x as f32 + 1.0,
                        y as f32,
                        1.0,
                    ))),
                )],
                &mut [],
            );
        }
    }
}
