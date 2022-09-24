use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_math::{EulerRot, Mat4, Vec3, Vec4, Vec4Swizzles};
use ard_render::{
    camera::{Camera, CameraDescriptor},
    factory::{Factory, ShaderCreateInfo},
    material::{MaterialCreateInfo, MaterialInstanceCreateInfo},
    mesh::{MeshBounds, MeshCreateInfo, VertexLayout},
    renderer::{Model, PreRender, RenderLayer, Renderable, RendererSettings},
    static_geometry::{StaticGeometry, StaticRenderable, StaticRenderableHandle},
    *,
};
use ard_window::prelude::*;
use ard_winit::prelude::*;
use std::time::Instant;

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
}

impl CameraMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<(Write<MainCamera>,)>,
        res: Res<(Read<Factory>, Read<InputState>, Write<Windows>)>,
    ) {
        let res = res.get();
        let factory = res.0.unwrap();
        let input = res.1.unwrap();
        let mut windows = res.2.unwrap();
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

        // Lock cursor
        if input.key_up(Key::M) {
            self.cursor_locked = !self.cursor_locked;

            let window = windows.get_mut(WindowId::primary()).unwrap();

            window.set_cursor_lock_mode(self.cursor_locked);
            window.set_cursor_visibility(!self.cursor_locked);
        }

        // Update the camera
        factory.update_camera(
            &main_camera.0,
            CameraDescriptor {
                position: self.position,
                target: self.position + forward.xyz(),
                up: up.xyz(),
                far: 1000.0,
                ..Default::default()
            },
        );
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

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Disable frame rate limit
    app.resources
        .get_mut::<RendererSettings>()
        .unwrap()
        .render_time = None;

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
    let mut camera_entity = [Entity::null()];
    app.world
        .entities_mut()
        .commands()
        .create((vec![MainCamera(camera)],), &mut camera_entity);

    // Create the camera system
    app.dispatcher.add_system(CameraMover {
        cursor_locked: false,
        look_speed: 0.1,
        move_speed: 30.0,
        entity: camera_entity[0],
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
    });

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
}
