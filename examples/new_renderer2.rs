use ard_core::prelude::*;
use ard_math::*;
use ard_pal::prelude::*;
use ard_render2::{factory::Factory, RenderPlugin, RendererSettings};
use ard_render_camera::{Camera, CameraClearColor};
use ard_render_meshes::{mesh::MeshCreateInfo, vertices::VertexAttributes};
use ard_render_objects::{objects::StaticDirty, Model, RenderFlags, RenderingMode};
use ard_window::prelude::*;
use ard_winit::prelude::*;

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Renderer Testing"),
                resizable: true,
                width: 1280.0,
                height: 720.0,
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                render_scene: true,
                render_time: None,
                present_mode: PresentMode::Immediate,
                render_scale: 1.0,
                canvas_size: None,
            },
            debug: true,
        })
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Create a mesh
    let mesh = factory
        .create_mesh(MeshCreateInfo {
            debug_name: Some("triangle".to_owned()),
            vertices: VertexAttributes {
                positions: &[
                    Vec4::new(1.0, 0.0, 0.1, 1.0),
                    Vec4::new(-1.0, 0.0, 0.1, 1.0),
                    Vec4::new(0.0, 1.0, 0.1, 1.0),
                ],
                normals: &[
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                ],
                tangents: None,
                colors: None,
                uv0: None,
                uv1: None,
                uv2: None,
                uv3: None,
            },
            indices: [0u32, 1, 2].as_slice(),
        })
        .unwrap();

    // Create a material
    let material = factory.create_pbr_material_instance().unwrap();

    // Create a camera
    app.world.entities().commands().create(
        (
            vec![Camera {
                near: 0.03,
                far: 250.0,
                fov: 80.0_f32.to_radians(),
                order: 0,
                clear_color: CameraClearColor::Color(Vec4::ZERO),
                flags: RenderFlags::empty(),
            }],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0)))],
        ),
        &mut [],
    );

    // Create a renderable object
    app.world.entities().commands().create(
        (
            vec![mesh],
            vec![material],
            vec![Model(Mat4::IDENTITY)],
            vec![RenderingMode::Opaque],
            vec![RenderFlags::empty()],
            vec![Static],
        ),
        &mut [],
    );

    app.resources.get_mut::<StaticDirty>().unwrap().mark();
}
