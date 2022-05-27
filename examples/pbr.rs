#[path = "./util.rs"]
mod util;

use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, math::*,
    window::prelude::*,
};

use ard_engine::graphics_assets::prelude as graphics_assets;

use ard_graphics_assets::prelude::PbrMaterialData;
use util::{CameraMovement, FrameRate, MainCameraState};

#[derive(SystemState)]
struct BoundingBoxSystem {
    material: Material,
    timer: f32,
}

impl BoundingBoxSystem {
    fn pre_render(
        &mut self,
        pre_render: PreRender,
        commands: Commands,
        queries: Queries<(Read<Model>, Read<PointLight>)>,
        res: Res<(Read<DebugDrawing>, Read<Factory>, Read<MainCameraState>)>,
    ) {
        let res = res.get();
        let draw = res.0.unwrap();
        let factory = res.1.unwrap();
        let camera_state = res.2.unwrap();

        self.timer += pre_render.0.as_secs_f32();

        factory.update_material_data(
            &self.material,
            bytemuck::bytes_of(&PbrMaterialData {
                base_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
                metallic: (self.timer.sin() * 0.5) + 0.5,
                roughness: 0.2,
            }),
        );
    }
}

impl Into<System> for BoundingBoxSystem {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(BoundingBoxSystem::pre_render)
            .run_after::<PreRender, CameraMovement>()
            .build()
    }
}

#[derive(Resource)]
struct CameraHolder {
    _camera: Camera,
}

#[tokio::main]
async fn main() {
    AppBuilder::new(ard_log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("PBR Scene"),
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
        .add_plugin(AssetsPlugin)
        .add_plugin(graphics_assets::GraphicsAssetsPlugin)
        .add_startup_function(setup)
        .add_resource(MainCameraState(CameraDescriptor {
            far: 200.0,
            ..Default::default()
        }))
        .add_system(CameraMovement {
            position: Vec3::new(0.0, 5.0, 15.0),
            rotation: Vec3::new(0.0, 180.0, 0.0),
            near: 0.3,
            far: 200.0,
            fov: (90.0 as f32).to_radians(),
            look_speed: 0.1,
            move_speed: 30.0,
            cursor_locked: false,
        })
        .add_system(FrameRate::default())
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();

    // Disable frame rate limit
    app.resources
        .get_mut::<RendererSettings>()
        .unwrap()
        .render_time = None;

    // Load the model
    let model_handle = assets.load::<graphics_assets::ModelAsset>(AssetName::new("test_scene.mdl"));
    assets.wait_for_load(&model_handle);

    // Insert the model into the world
    let model = assets.get(&model_handle).unwrap();

    app.dispatcher.add_system(BoundingBoxSystem {
        material: model.materials[5].clone(),
        timer: 0.0,
    });

    // NOTE: Unless you want the draw to exist forever, you should store the handles generated here
    // so they can be unregistered later
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();

    for node in &model.nodes {
        let mesh_group = &model.mesh_groups[node.mesh_group];
        for (mesh, material_idx) in &mesh_group.meshes {
            let material = &model.materials[*material_idx];

            draws.register(
                &[(
                    Renderable {
                        mesh: mesh.clone(),
                        material: material.clone(),
                    },
                    Model(node.transform),
                )],
                &mut [],
            );
        }
    }

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

    // Create lights
    const LIGHT_COUNT: usize = 1;
    const LIGHT_RING_RADIUS: f32 = 0.0;
    const LIGHT_RANGE: f32 = 80.0;
    const LIGHT_INTENSITY: f32 = 6.0;

    let mut lights = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for i in 0..LIGHT_COUNT {
        let angle = (i as f32 / LIGHT_COUNT as f32) * 2.0 * std::f32::consts::PI;
        let pos = Vec3::new(0.0, 32.0, 0.0)
            + (Vec3::new(angle.sin(), 0.0, angle.cos()) * LIGHT_RING_RADIUS);

        lights.0.push(Model(Mat4::from_translation(pos)));
        lights.1.push(PointLight {
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: LIGHT_INTENSITY,
            radius: LIGHT_RANGE,
        });
    }

    app.world.entities().commands().create(lights, &mut []);
}
