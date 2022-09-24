use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec3, Vec4};
use ard_render::{
    camera::{Camera, CameraDescriptor},
    factory::{Factory, ShaderCreateInfo},
    material::{MaterialCreateInfo, MaterialInstanceCreateInfo},
    mesh::{MeshBounds, MeshCreateInfo, VertexLayout},
    renderer::{Model, RenderLayer, Renderable},
    *,
};
use ard_window::prelude::*;
use ard_winit::prelude::*;

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Error)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Test Window"),
                resizable: true,
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            debug: true,
        })
        .add_startup_function(setup)
        .run();
}

#[derive(Component)]
struct MainCamera(Camera);

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Load in the shaders
    let vshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/example/new_rend.vert.spv"),
            debug_name: None,
        })
        .unwrap();
    let fshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/example/new_rend.frag.spv"),
            debug_name: None,
        })
        .unwrap();

    // Create the material
    let material = factory.create_material(MaterialCreateInfo {
        vertex_shader: vshd,
        fragment_shader: fshd,
        vertex_layout: VertexLayout::COLOR,
        texture_count: 0,
        data_size: 0,
    });

    let material_instance =
        factory.create_material_instance(MaterialInstanceCreateInfo { material });

    // Create the triangle mesh
    let mesh = factory.create_mesh(MeshCreateInfo {
        bounds: MeshBounds::Generate,
        indices: &[0, 1, 2, 0, 2, 1],
        positions: &[
            Vec4::new(1.0, 0.0, 0.5, 1.0),
            Vec4::new(0.0, 1.0, 0.5, 1.0),
            Vec4::new(-1.0, 0.0, 0.5, 1.0),
        ],
        normals: None,
        tangents: None,
        colors: Some(&[
            Vec4::new(1.0, 0.0, 0.0, 1.0),
            Vec4::new(0.0, 1.0, 0.0, 1.0),
            Vec4::new(0.0, 0.0, 1.0, 1.0),
        ]),
        uv0: None,
        uv1: None,
        uv2: None,
        uv3: None,
    });

    // Create the main camera
    let camera = factory.create_camera(CameraDescriptor::default());
    app.world
        .entities_mut()
        .commands()
        .create((vec![MainCamera(camera)],), &mut []);

    // Create the triangle object
    app.world.entities_mut().commands().create(
        (
            vec![Renderable {
                mesh,
                material: material_instance,
                layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
            }],
            vec![Model(Mat4::from_translation(Vec3::Z))],
        ),
        &mut [],
    );
}
