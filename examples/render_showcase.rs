use std::time::Instant;

use ard_assets::prelude::{AssetName, Assets, AssetsPlugin};
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_math::*;
use ard_pal::prelude::*;
use ard_render::{
    factory::Factory, system::PostRender, AntiAliasingMode, RenderPlugin, RendererSettings,
};
use ard_render_assets::{model::ModelAsset, RenderAssetsPlugin};
use ard_render_camera::{Camera, CameraClearColor};
use ard_render_lighting::{global::GlobalLighting, Light};
use ard_render_meshes::{mesh::MeshCreateInfo, vertices::VertexAttributes};
use ard_render_objects::{Model, RenderFlags, RenderingMode};
use ard_render_pbr::PbrMaterialData;
use ard_window::prelude::*;
use ard_winit::prelude::*;

#[derive(SystemState)]
pub struct FrameRate {
    frame_ctr: usize,
    last_sec: Instant,
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate {
            frame_ctr: 0,
            last_sec: Instant::now(),
        }
    }
}

impl FrameRate {
    fn post_render(&mut self, _: PostRender, _: Commands, _: Queries<()>, _: Res<()>) {
        let now = Instant::now();
        self.frame_ctr += 1;
        if now.duration_since(self.last_sec).as_secs_f32() >= 1.0 {
            println!("Frame Rate: {}", self.frame_ctr);
            self.last_sec = now;
            self.frame_ctr = 0;
        }
    }
}

impl Into<System> for FrameRate {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(FrameRate::post_render)
            .build()
    }
}

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

#[derive(SystemState)]
pub struct SunMover {
    pub speed: f32,
    pub time: f32,
}

impl SunMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<GlobalLighting>,)>,
    ) {
        self.time += evt.0.as_secs_f32();

        let mut lighting = res.get_mut::<GlobalLighting>().unwrap();

        let dir = Vec4::new(0.0, 0.0, 1.0, 1.0);

        lighting.set_sun_direction(
            (Mat4::from_rotation_x((self.time * self.speed).to_radians()) * dir).xyz(),
        );
    }
}

impl From<SunMover> for System {
    fn from(mover: SunMover) -> Self {
        SystemBuilder::new(mover)
            .with_handler(SunMover::on_tick)
            .build()
    }
}

fn main() {
    let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
    let _puffin_server = puffin_http::Server::new(&server_addr).unwrap();
    puffin::set_scopes_on(true);

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
                anti_aliasing: AntiAliasingMode::MSAA(MultiSamples::Count8),
                render_scale: 1.0,
                canvas_size: None,
            },
            debug: false,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_system(FrameRate::default())
        .add_system(SunMover {
            speed: 10.0,
            time: 0.0,
        })
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

    /*
    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(-150.0, 0.0, 0.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(150.0, 0.0, 0.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(0.0, 0.0, 200.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(0.0, 0.0, -200.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );
    */

    // Big quad for the floor to prevent light leaking when the sun is low.
    let quad = factory
        .create_mesh(MeshCreateInfo {
            debug_name: Some("quad".to_owned()),
            vertices: VertexAttributes {
                positions: &[
                    Vec4::new(-1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, -1.0, 1.0),
                ],
                normals: &[
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                ],
                tangents: None,
                colors: None,
                uv0: None,
                uv1: None,
                uv2: None,
                uv3: None,
            },
            indices: [1u32, 0, 2, 2, 0, 3, 4, 5, 6, 4, 6, 7].as_slice(),
        })
        .unwrap();

    let quad_material = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &quad_material,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    app.world.entities().commands().create(
        (
            vec![quad.clone()],
            vec![quad_material.clone()],
            vec![Model(
                Mat4::from_scale(Vec3::new(1000.0, 1.0, 1000.0))
                    * Mat4::from_translation(Vec3::new(0.0, -3.0, 0.0)),
            )],
            vec![RenderingMode::Opaque],
            vec![RenderFlags::empty()],
            vec![Static(0)],
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
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 1.0, 1.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let red = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &red,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 0.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let green = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &green,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(0.0, 1.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let blue = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &blue,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
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
                far: 200.0,
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
        move_speed: 24.0,
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

    // Gimmie a ton of light
    use rand::prelude::*;

    const LIGHT_COUNT: usize = 200;
    const LIGHT_AREA_MIN: Vec3 = Vec3::new(-20.0, 0.0, -25.0);
    const LIGHT_AREA_MAX: Vec3 = Vec3::new(20.0, 30.0, 25.0);
    let mut rng = rand::thread_rng();
    let mut lights = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for _ in 0..LIGHT_COUNT {
        lights.0.push(Light::Point {
            color: Vec3::new(
                (rng.gen::<f32>() * 0.5) + 0.5,
                (rng.gen::<f32>() * 0.5) + 0.5,
                (rng.gen::<f32>() * 0.5) + 0.5,
            ),
            range: 1.0,
            intensity: 64.0,
        });

        lights.1.push(Model(Mat4::from_translation(Vec3::new(
            rng.gen_range(LIGHT_AREA_MIN.x..LIGHT_AREA_MAX.x),
            rng.gen_range(LIGHT_AREA_MIN.y..LIGHT_AREA_MAX.y),
            rng.gen_range(LIGHT_AREA_MIN.z..LIGHT_AREA_MAX.z),
        ))));
    }

    app.world.entities().commands().create(lights, &mut []);

    app.resources.get_mut::<DirtyStatic>().unwrap().signal(0);
    app.resources.get_mut::<DirtyStatic>().unwrap().signal(1);
}
