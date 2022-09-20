#[path = "./util.rs"]
mod util;

use std::time::Instant;

use ard_engine::{
    core::prelude::*, ecs::prelude::*, graphics::prelude::*, math::*, window::prelude::*,
};

use util::{CameraMovement, FrameRate, MainCameraState};

#[derive(Resource)]
struct CameraHolder {
    _camera: Camera,
}
#[derive(SystemState)]
struct SpinningCamera {
    camera_rot: f32,
    frame_ctr: usize,
    last_sec: Instant,
}

impl Default for SpinningCamera {
    fn default() -> Self {
        Self {
            camera_rot: 0.0,
            frame_ctr: 0,
            last_sec: Instant::now(),
        }
    }
}

impl SpinningCamera {
    fn tick(&mut self, tick: Tick, _: Commands, _: Queries<()>, res: Res<(Write<Factory>,)>) {
        const SPIN_SPEED: f32 = 0.2;
        const CAM_DISTANCE: f32 = 150.0;

        let dt = tick.0.as_secs_f32();
        self.camera_rot += dt * SPIN_SPEED;

        let res = res.get();
        let factory = res.0.unwrap();
        let main_camera = factory.main_camera();

        let position = Vec3::new(
            self.camera_rot.sin() * CAM_DISTANCE,
            5.0,
            self.camera_rot.cos() * CAM_DISTANCE,
        );

        let descriptor = CameraDescriptor {
            position,
            far: 500.0,
            ..Default::default()
        };

        factory.update_camera(&main_camera, descriptor);
    }

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

impl Into<System> for SpinningCamera {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(SpinningCamera::tick)
            .with_handler(SpinningCamera::pre_render)
            .build()
    }
}

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Error)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("Many Cubes"),
                vsync: false,
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(VkGraphicsPlugin {
            context_create_info: GraphicsContextCreateInfo {
                window: WindowId::primary(),
                debug: false,
            },
        })
        .add_startup_function(setup)
        .add_resource(MainCameraState::default())
        .add_system(CameraMovement {
            position: Vec3::new(0.0, 5.0, 150.0),
            rotation: Vec3::new(0.0, 180.0, 0.0),
            near: 0.3,
            far: 200.0,
            fov: (80.0 as f32).to_radians(),
            look_speed: 0.1,
            move_speed: 30.0,
            cursor_locked: false,
        })
        .add_system(FrameRate::default())
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Disable frame rate limit
    app.resources
        .get_mut::<RendererSettings>()
        .unwrap()
        .render_time = None;

    // Shaders
    let create_info = ShaderCreateInfo {
        ty: ShaderType::Fragment,
        code: include_bytes!("./assets/example/triangle.frag.spv"),
        vertex_layout: VertexLayout::default(),
        inputs: ShaderInputs {
            ubo_size: 0,
            texture_count: 0,
        },
    };

    let frag_shader = factory.create_shader(&create_info);

    let create_info = ShaderCreateInfo {
        ty: ShaderType::Vertex,
        code: include_bytes!("./assets/example/triangle.vert.spv"),
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
        use_depth_buffer: true,
        use_occlusion_culling: true,
    };

    let pipeline = factory.create_pipeline(&create_info);

    // Material
    let create_info = MaterialCreateInfo { pipeline };

    let material = factory.create_material(&create_info);

    // Mesh
    let positions = [
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, 0.5, 0.0),
        Vec4::new(0.5, -0.5, 0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, -0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, 0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, 0.5, 0.0),
        Vec4::new(0.5, -0.5, 0.5, 0.0),
        Vec4::new(-0.5, -0.5, 0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 1.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
    ];

    let colors = [
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 1.0, 1.0, 1.0),
    ];

    let indices: Vec<u32> = (0u32..42u32).collect();

    let create_info = MeshCreateInfo {
        positions: &positions,
        colors: Some(&colors),
        indices: &indices,
        bounds: MeshBounds::Generate,
        ..Default::default()
    };

    let mesh = factory.create_mesh(&create_info);

    let create_info = CameraCreateInfo {
        descriptor: CameraDescriptor {
            position: Vec3::new(100.0, 50.0, 100.0),
            center: Vec3::new(0.0, 0.0, 0.0),
            far: 500.0,
            ..Default::default()
        },
    };

    app.resources.add(CameraHolder {
        _camera: factory.create_camera(&create_info),
    });

    // Register draws
    // NOTE: Unless you want the draw to exist forever, you should store the handles generated here
    // so they can be unregistered later
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();

    const SPACING: f32 = 0.95;
    const WIDTH: usize = 100;
    const DEPTH: usize = 100;
    const HEIGHT: usize = 100;

    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            for z in 0..DEPTH {
                let position = Vec3::new(
                    x as f32 - (WIDTH as f32 / 2.0),
                    y as f32 - (HEIGHT as f32 / 2.0),
                    z as f32 - (DEPTH as f32 / 2.0),
                ) * SPACING;

                let model = Mat4::from_translation(position);

                draws.register(
                    &[StaticRenderable {
                        renderable: Renderable {
                            mesh: mesh.clone(),
                            material: material.clone(),
                            layers: RenderLayerFlags::all(),
                        },
                        model: Model(model),
                        entity: Entity::null(),
                    }],
                    &mut [],
                );
            }
        }
    }
}
