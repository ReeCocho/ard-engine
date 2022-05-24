use std::time::Instant;

use ard_engine::{
    core::prelude::*,
    ecs::prelude::*,
    graphics::prelude::*,
    math::{Mat4, Vec3, Vec4},
    window::prelude::*,
};

#[derive(Resource)]
struct CameraHolder {
    _camera: Camera,
}

#[derive(SystemState)]
struct SpinningCubes {
    rot: f32,
    frame_ctr: usize,
    last_sec: Instant,
}

impl Default for SpinningCubes {
    fn default() -> Self {
        Self {
            rot: 0.0,
            frame_ctr: 0,
            last_sec: Instant::now(),
        }
    }
}

impl SpinningCubes {
    fn tick(
        &mut self,
        tick: Tick,
        _: Commands,
        queries: Queries<(Write<Model>,)>,
        res: Res<(Write<Factory>,)>,
    ) {
        const SPIN_SPEED: f32 = 0.2;
        const SPIN_RADIUS: f32 = 5.0;

        let dt = tick.0.as_secs_f32();
        self.rot += dt * SPIN_SPEED;

        let res = res.get();
        let factory = res.0.unwrap();
        let main_camera = factory.main_camera();

        let position = Vec3::new(0.0, 0.0, -10.0);

        let descriptor = CameraDescriptor {
            position,
            far: 500.0,
            ..Default::default()
        };

        factory.update_camera(&main_camera, descriptor);

        // Rotate cubes
        let qry = queries.make::<(Write<Model>,)>();
        let rads_per_cube = (2.0 * std::f32::consts::PI) / qry.len() as f32;
        for (i, (model,)) in qry.into_iter().enumerate() {
            let rot_offset = self.rot + (i as f32 * rads_per_cube);
            model.0 = Mat4::from_translation(Vec3::new(
                rot_offset.cos() * SPIN_RADIUS,
                rot_offset.sin() * SPIN_RADIUS,
                0.0,
            ));
        }
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

impl Into<System> for SpinningCubes {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(SpinningCubes::tick)
            .with_handler(SpinningCubes::pre_render)
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
                title: String::from("Spinning Cubes"),
                vsync: false,
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
        .add_system(SpinningCubes::default())
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
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, -0.5, -0.5, 0.0),
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
        Vec4::new(0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, -0.5, 0.0),
        Vec4::new(0.5, -0.5, 0.5, 0.0),
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
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
        Vec4::new(0.5, 0.5, -0.5, 1.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, 0.5, 0.0),
        Vec4::new(-0.5, 0.5, -0.5, 0.0),
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
            position: Vec3::new(0.0, 5.0, 150.0),
            center: Vec3::new(0.0, 0.0, 0.0),
            far: 500.0,
            ..Default::default()
        },
    };

    app.resources.add(CameraHolder {
        _camera: factory.create_camera(&create_info),
    });

    // Create centeral static cube
    let static_geo = app.resources.get_mut::<StaticGeometry>().unwrap();
    static_geo.register(
        &[(
            Renderable {
                mesh: mesh.clone(),
                material: material.clone(),
            },
            Model(Mat4::IDENTITY),
        )],
        &mut [],
    );

    // Create cubes
    const CUBE_COUNT: usize = 8;

    let cubes = (
        vec![
            Renderable::<VkBackend> {
                mesh: mesh.clone(),
                material: material.clone(),
            };
            CUBE_COUNT
        ],
        vec![Model(Mat4::IDENTITY); CUBE_COUNT],
    );

    app.world.entities().commands().create(cubes, &mut []);
}
