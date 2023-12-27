use ard_assets::prelude::{AssetName, Assets, AssetsPlugin};
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_math::*;
use ard_pal::prelude::*;
use ard_render2::{factory::Factory, RenderPlugin, RendererSettings};
use ard_render_assets::{model::ModelAsset, RenderAssetsPlugin};
use ard_render_camera::{Camera, CameraClearColor};
use ard_render_lighting::Light;
use ard_render_meshes::{mesh::MeshCreateInfo, vertices::VertexAttributes};
use ard_render_objects::{Model, RenderFlags, RenderingMode};
use ard_render_pbr::PbrMaterialData;
use ard_window::prelude::*;
use ard_winit::prelude::*;

//#[path = "./util.rs"]
//mod util;

#[derive(SystemState)]
pub struct CameraMover {
    pub cursor_locked: bool,
    pub look_speed: f32,
    pub move_speed: f32,
    pub entity: Entity,
    pub position: Vec3,
    pub rotation: Vec3,
}

impl CameraMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<(Write<Camera>, Write<Model>)>,
        res: Res<(Read<Factory>, Read<InputState>, Write<Windows>)>,
    ) {
        let input = res.get::<InputState>().unwrap();
        let mut windows = res.get_mut::<Windows>().unwrap();

        // Rotate the camera
        let delta = evt.0.as_secs_f32();
        if self.cursor_locked {
            let (mx, my) = input.mouse_delta();
            self.rotation.x += (my as f32) * self.look_speed;
            self.rotation.y += (mx as f32) * self.look_speed;
            self.rotation.x = self.rotation.x.clamp(-85.0, 85.0);
        }

        // Direction from rotation
        let rot = Mat4::from_euler(
            EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            0.0,
        );

        // Move the camera
        let right = rot.col(0);
        let forward = rot.col(2);

        if self.cursor_locked {
            if input.key(Key::W) {
                self.position += forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::S) {
                self.position -= forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::A) {
                self.position -= right.xyz() * delta * self.move_speed;
            }

            if input.key(Key::D) {
                self.position += right.xyz() * delta * self.move_speed;
            }
        }

        // Lock cursor
        if input.key_up(Key::M) {
            self.cursor_locked = !self.cursor_locked;

            let window = windows.get_mut(WindowId::primary()).unwrap();

            window.set_cursor_lock_mode(self.cursor_locked);
            window.set_cursor_visibility(!self.cursor_locked);
        }

        // Update the camera
        let mut query = queries.get::<Write<Model>>(self.entity).unwrap();
        query.0 = Mat4::from_translation(self.position) * rot;
    }
}

impl From<CameraMover> for System {
    fn from(mover: CameraMover) -> Self {
        SystemBuilder::new(mover)
            .with_handler(CameraMover::on_tick)
            .build()
    }
}

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
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                render_scene: true,
                render_time: None,
                present_mode: PresentMode::Mailbox,
                render_scale: 1.0,
                canvas_size: None,
            },
            debug: true,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();

    // Load in a model
    let model = assets.load::<ModelAsset>(AssetName::new("test_scene.model"));
    assets.wait_for_load(&model);

    // Instantiate models
    let instance = assets.get(&model).unwrap().instantiate();

    println!("{}", instance.meshes.meshes.len() * 3);

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

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
            indices: [0u32, 1, 2, 0, 2, 1].as_slice(),
        })
        .unwrap();

    // Create a material
    let material = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &material,
        &PbrMaterialData {
            alpha_cutoff: 1.0,
            color: Vec4::new(1.0, 1.0, 1.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let red = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &red,
        &PbrMaterialData {
            alpha_cutoff: 1.0,
            color: Vec4::new(1.0, 0.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let green = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &green,
        &PbrMaterialData {
            alpha_cutoff: 1.0,
            color: Vec4::new(0.0, 1.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let blue = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &blue,
        &PbrMaterialData {
            alpha_cutoff: 1.0,
            color: Vec4::new(0.0, 0.0, 1.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    // Create a camera
    let mut camera = [Entity::null()];
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
        &mut camera,
    );

    app.dispatcher.add_system(CameraMover {
        cursor_locked: false,
        look_speed: 0.1,
        move_speed: 15.0,
        entity: camera[0],
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
    });

    // Create a renderable object
    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![material.clone()],
            vec![Model(Mat4::IDENTITY)],
            vec![RenderingMode::Opaque],
            vec![RenderFlags::empty()],
            vec![Static(0)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![material.clone()],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)))],
            vec![RenderingMode::AlphaCutout],
            vec![RenderFlags::empty()],
            vec![Static(0)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone(), mesh.clone()],
            vec![red, green],
            vec![
                Model(Mat4::from_translation(Vec3::new(0.0, -1.0, 1.0))),
                Model(Mat4::from_translation(Vec3::new(0.0, -1.0, -1.0))),
            ],
            vec![RenderingMode::Transparent, RenderingMode::Transparent],
            vec![RenderFlags::empty(), RenderFlags::empty()],
            vec![Static(1), Static(1)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![blue],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)))],
            vec![RenderingMode::Transparent],
            vec![RenderFlags::empty()],
        ),
        &mut [],
    );

    // Gimmie a light
    app.world.entities().commands().create(
        (
            vec![Light::Point {
                color: Vec3::ONE,
                range: 8.0,
                intensity: 8.0,
            }],
            vec![Model(Mat4::IDENTITY)],
        ),
        &mut [],
    );

    app.resources.get_mut::<DirtyStatic>().unwrap().signal(0);
    app.resources.get_mut::<DirtyStatic>().unwrap().signal(1);
}
