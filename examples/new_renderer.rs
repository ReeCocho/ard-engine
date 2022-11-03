use ard_assets::prelude::*;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_formats::mesh::VertexLayout;
use ard_input::{InputState, Key};
use ard_math::{EulerRot, Mat4, Vec3, Vec4, Vec4Swizzles};
use ard_pal::prelude::{CullMode, FrontFace, PresentMode};
use ard_render::{
    asset::{cube_map::CubeMapAsset, model::ModelAsset, RenderAssetsPlugin},
    camera::{Camera, CameraClearColor, CameraDescriptor, CameraIbl, CameraShadows},
    factory::{Factory, ShaderCreateInfo},
    lighting::PointLight,
    material::{MaterialCreateInfo, MaterialInstanceCreateInfo},
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    renderer::{
        gui::{Gui, View},
        Model, PreRender, RenderLayer, Renderable, RendererSettings,
    },
    static_geometry::{StaticGeometry, StaticRenderableHandle},
    *,
};
use ard_window::prelude::*;
use ard_winit::prelude::*;
use rand::Rng;
use std::time::Instant;

#[path = "./util.rs"]
mod util;

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Warn)
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
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                present_mode: PresentMode::Immediate,
                ..Default::default()
            },
            debug: false,
        })
        .add_plugin(RenderAssetsPlugin {
            pbr_material: AssetNameBuf::from("pbr.mat"),
        })
        .add_system(FrameRate::default())
        .add_startup_function(setup)
        .run();
}

#[derive(Component)]
struct MainCamera(Camera);

#[derive(Resource)]
struct StaticHandles(Vec<StaticRenderableHandle>);

#[derive(SystemState)]
struct CameraMover {
    pub cursor_locked: bool,
    pub look_speed: f32,
    pub move_speed: f32,
    pub entity: Entity,
    pub position: Vec3,
    pub rotation: Vec3,
    pub descriptor: CameraDescriptor,
}

impl CameraMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<(Write<MainCamera>,)>,
        res: Res<(Read<Factory>, Read<InputState>, Write<Windows>)>,
    ) {
        let factory = res.get::<Factory>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let mut windows = res.get_mut::<Windows>().unwrap();
        let main_camera = queries.get::<Write<MainCamera>>(self.entity).unwrap();

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
        let up = rot.col(1);
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

        // Toggle AO
        if input.key_up(Key::O) {
            self.descriptor.ao = !self.descriptor.ao;
        }

        // Update the camera
        self.descriptor.position = self.position;
        self.descriptor.target = self.position + forward.xyz();
        self.descriptor.up = up.xyz();
        self.descriptor.near = 0.1;
        self.descriptor.far = 100.0;
        factory.update_camera(&main_camera.0, self.descriptor.clone());
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
    fn pre_render(&mut self, _: PreRender, _: Commands, _: Queries<()>, _: Res<()>) {
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
            .with_handler(FrameRate::pre_render)
            .build()
    }
}

struct Settings {
    x: String,
}

impl View for Settings {
    fn show(
        &mut self,
        ctx: &egui::Context,
        _: &Commands,
        _: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut settings = res.get_mut::<RendererSettings>().unwrap();
        egui::Window::new("Settings").show(ctx, |ui| {
            ui.add(
                egui::Slider::new(&mut settings.post_processing.exposure, 0.0..=1.0)
                    .step_by(0.01)
                    .text("Exposure"),
            );
            ui.toggle_value(&mut settings.post_processing.fxaa, "FXAA");
            ui.text_edit_singleline(&mut self.x);
        });
    }
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();
    let static_geo = app.resources.get::<StaticGeometry>().unwrap();
    let mut gui = app.resources.get_mut::<Gui>().unwrap();

    // Disable frame rate limit
    let mut settings = app.resources.get_mut::<RendererSettings>().unwrap();
    settings.render_time = None;
    settings.render_scale = 1.0;

    // Add in GUI views
    gui.add_view(Settings {
        x: String::default(),
    });

    //*
    // Load in the scene
    let model_handle = assets.load::<ModelAsset>(AssetName::new("test_scene.model"));
    let sky_box_handle = assets.load::<CubeMapAsset>(AssetName::new("sky_box.cube"));
    let diffuse_irradiance_handle =
        assets.load::<CubeMapAsset>(AssetName::new("diffuse_irradiance.cube"));
    let prefiltered_env_handle =
        assets.load::<CubeMapAsset>(AssetName::new("prefiltered_env.cube"));
    assets.wait_for_load(&model_handle);
    assets.wait_for_load(&sky_box_handle);
    assets.wait_for_load(&diffuse_irradiance_handle);
    assets.wait_for_load(&prefiltered_env_handle);
    //*/
    // Create the main camera
    let camera_descriptor = CameraDescriptor {
        shadows: Some(CameraShadows {
            resolution: 4096,
            cascades: 4,
        }),
        clear_color: CameraClearColor::SkyBox(
            assets.get(&sky_box_handle).unwrap().cube_map.clone(),
        ),
        ibl: CameraIbl {
            diffuse_irradiance: Some(
                assets
                    .get(&diffuse_irradiance_handle)
                    .unwrap()
                    .cube_map
                    .clone(),
            ),
            prefiltered_environment: Some(
                assets
                    .get(&prefiltered_env_handle)
                    .unwrap()
                    .cube_map
                    .clone(),
            ),
        },
        ..Default::default()
    };
    let camera = factory.create_camera(camera_descriptor.clone());
    let mut camera_entity = [Entity::null()];
    app.world
        .entities_mut()
        .commands()
        .create((vec![MainCamera(camera)],), &mut camera_entity);

    // Create the camera system
    app.dispatcher.add_system(CameraMover {
        cursor_locked: false,
        look_speed: 0.1,
        move_speed: 8.0,
        entity: camera_entity[0],
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
        descriptor: camera_descriptor,
    });

    // Instantiate the model
    let asset = assets.get(&model_handle).unwrap();
    let (handles, _) = asset.instantiate_static(&static_geo, app.world.entities().commands());
    app.resources.add(StaticHandles(handles));

    ///*
    // Create light cube data
    let vshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/new_render/color.vert.spv"),
            debug_name: None,
        })
        .unwrap();
    let fshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/new_render/color.frag.spv"),
            debug_name: None,
        })
        .unwrap();

    let material = factory.create_material(MaterialCreateInfo {
        vertex_shader: vshd,
        depth_only_shader: None,
        fragment_shader: fshd,
        vertex_layout: VertexLayout::empty(),
        texture_count: 0,
        data_size: 0,
        cull_mode: CullMode::None,
        front_face: FrontFace::Clockwise,
    });

    let material_instance =
        factory.create_material_instance(MaterialInstanceCreateInfo { material });

    let mesh = factory.create_mesh(MeshCreateInfo {
        bounds: MeshBounds::Generate,
        indices: &util::CUBE_INDICES,
        vertices: Vertices::Attributes {
            positions: &util::CUBE_VERTICES,
            normals: None,
            tangents: None,
            colors: None,
            uv0: None,
            uv1: None,
            uv2: None,
            uv3: None,
        },
    });

    // Create some random lights
    const LIGHT_COUNT: usize = 4096 * 2;
    const LIGHT_SPACING: (f32, f32, f32) = (32.0, 16.0, 24.0);
    const LIGHT_OFFSET: (f32, f32, f32) = (0.0, 10.0, 0.0);
    const LIGHT_RANGE: (f32, f32) = (1.0, 2.0);
    const LIGHT_INTENSITY: (f32, f32) = (6.0, 12.0);

    let mut rng = rand::thread_rng();

    let mut light_pack = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for i in 0..LIGHT_COUNT {
        let t = Vec3::new(
            rng.gen_range(-LIGHT_SPACING.0..=LIGHT_SPACING.0) + LIGHT_OFFSET.0,
            rng.gen_range(-LIGHT_SPACING.1..=LIGHT_SPACING.1) + LIGHT_OFFSET.1,
            rng.gen_range(-LIGHT_SPACING.2..=LIGHT_SPACING.2) + LIGHT_OFFSET.2,
        );
        let model = Mat4::from_translation(t) * Mat4::from_scale(Vec3::new(0.1, 0.1, 0.1));

        light_pack.0.push(Model(model));
        light_pack.1.push(PointLight {
            color: Vec3::new(
                if i % 3 == 0 { 1.0 } else { 0.0 },
                if i % 3 == 1 { 1.0 } else { 0.0 },
                if i % 3 == 2 { 1.0 } else { 0.0 },
            ),
            intensity: rng.gen_range(LIGHT_INTENSITY.0..=LIGHT_INTENSITY.1),
            range: rng.gen_range(LIGHT_RANGE.0..=LIGHT_RANGE.1),
        });
        light_pack.2.push(Renderable {
            mesh: mesh.clone(),
            material: material_instance.clone(),
            layers: RenderLayer::OPAQUE,
        });
    }

    app.world
        .entities_mut()
        .commands()
        .create(light_pack, &mut []);
    //*/

    /*
    // Create static triangle objects
    const WIDTH: usize = 16;
    const DEPTH: usize = 16;
    const HEIGHT: usize = 16;
    const SPACING: f32 = 2.0;

    let mut renderables = Vec::with_capacity(WIDTH * DEPTH * HEIGHT);
    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            for z in 0..DEPTH {
                let model =
                    Mat4::from_translation(Vec3::new(x as f32, y as f32, z as f32) * SPACING);

                renderables.push(StaticRenderable {
                    renderable: Renderable {
                        mesh: mesh.clone(),
                        material: material_instance.clone(),
                        layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                    },
                    model: Model(model),
                    entity: Entity::null(),
                });
            }
        }
    }

    let handles = app
        .resources
        .get::<StaticGeometry>()
        .unwrap()
        .register(&renderables);
    app.resources.add(StaticHandles(handles));

    // Create dynamic triangle objects
    let mut pack = (
        Vec::with_capacity(WIDTH * DEPTH * HEIGHT),
        Vec::with_capacity(WIDTH * DEPTH * HEIGHT),
    );

    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            for z in 0..DEPTH {
                let model = Mat4::from_translation(
                    Vec3::new(-(x as f32) - 1.0, y as f32, z as f32) * SPACING,
                );

                pack.0.push(Renderable {
                    mesh: mesh.clone(),
                    material: material_instance.clone(),
                    layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                });
                pack.1.push(Model(model));
            }
        }
    }

    app.world.entities_mut().commands().create(pack, &mut []);

    // Create point lights
    const LIGHT_COUNT: usize = 8;
    const LIGHT_SPACING: f32 = 4.0;
    const LIGHT_RANGE: f32 = 4.0;

    let mut pack = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for i in 0..LIGHT_COUNT {
        let t = Vec3::new(i as f32 * LIGHT_SPACING, 8.0, 8.0);
        let model = Mat4::from_translation(t);
        println!("{}", t.x);
        pack.0.push(Model(model));
        pack.1.push(PointLight {
            color: Vec3::ONE,
            intensity: 1.0,
            range: LIGHT_RANGE,
        });
    }

    app.world.entities_mut().commands().create(pack, &mut []);
    */
}
