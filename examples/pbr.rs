#[path = "./util.rs"]
mod util;

use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, math::*,
    window::prelude::*,
};

use ard_engine::graphics_assets::prelude as graphics_assets;

use util::{CameraMovement, FrameRate, MainCameraState};

#[derive(SystemState)]
struct BoundingBoxSystem {
    mesh: Mesh,
    models: [Mat4; 10],
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

        for (model, light) in queries.make::<(Read<Model>, Read<PointLight>)>() {
            let camera_ubo = CameraUBO::new(&camera_state.0, 1280.0, 720.0);
            let inv = camera_ubo.vp.inverse();

            let pos = model.0.col(3).xyz();
            let camera_pos = camera_state.0.center;
            let cam_to_center = pos - camera_pos;
            let dist_to_camera = cam_to_center.length();
            let clip_rad = light.radius * (1.0 / (camera_state.0.fov / 2.0).tan()) / dist_to_camera;

            for i in 0..2 {
                let mut end_pt = camera_ubo.vp * Vec4::new(pos.x, pos.y, pos.z, 1.0);
                end_pt /= end_pt.w;

                match i {
                    0 => end_pt.y += clip_rad,
                    1 => end_pt.y -= clip_rad,
                    _ => {}
                }

                end_pt = inv * end_pt;
                end_pt /= end_pt.w;
            }

            let TABLE_X = 32.0;
            let TABLE_Y = 16.0;
            let x = 16.0;
            let y = 8.0;

            let cluster_min_ss = Vec4::new(
                ((x / TABLE_X) * 2.0) - 1.0,
                ((y / TABLE_Y) * 2.0) - 1.0,
                0.0,
                1.0,
            );

            let cluster_max_ss = Vec4::new(
                (((x + 1.0) / TABLE_X) * 2.0) - 1.0,
                (((y + 1.0) / TABLE_Y) * 2.0) - 1.0,
                0.0,
                1.0,
            );

            // Finding the 4 intersection points made from each point to the cluster near/far plane
            let mut min_point_near = camera_ubo.projection_inv * cluster_min_ss;
            let mut min_point_far =
                camera_ubo.projection_inv * Vec4::new(cluster_min_ss.x, cluster_min_ss.y, 1.0, 1.0);
            let mut max_point_near = camera_ubo.projection_inv * cluster_max_ss;
            let mut max_point_far =
                camera_ubo.projection_inv * Vec4::new(cluster_max_ss.x, cluster_max_ss.y, 1.0, 1.0);

            min_point_near /= min_point_near.w;
            min_point_far /= min_point_far.w;
            max_point_near /= max_point_near.w;
            max_point_far /= max_point_far.w;

            // Min and max bounding area
            let mut min_point_AABB =
                min_point_near.min(min_point_far.min(max_point_near.min(max_point_far)));
            let mut max_point_AABB =
                min_point_near.max(min_point_far.max(max_point_near.max(max_point_far)));

            // Compute square distance from light to cluster volume
            let mut sq_dist = 0.0;
            if pos.x < min_point_AABB.x {
                sq_dist += (min_point_AABB.x - pos.x) * (min_point_AABB.x - pos.x);
            }
            if pos.x > max_point_AABB.x {
                sq_dist += (pos.x - max_point_AABB.x) * (pos.x - max_point_AABB.x);
            }
            if pos.y < min_point_AABB.y {
                sq_dist += (min_point_AABB.y - pos.y) * (min_point_AABB.y - pos.y);
            }
            if pos.y > max_point_AABB.y {
                sq_dist += (pos.y - max_point_AABB.y) * (pos.y - max_point_AABB.y);
            }
            if pos.z < min_point_AABB.z {
                sq_dist += (min_point_AABB.z - pos.z) * (min_point_AABB.z - pos.z);
            }
            if pos.z > max_point_AABB.z {
                sq_dist += (pos.z - max_point_AABB.z) * (pos.z - max_point_AABB.z);
            }

            min_point_AABB = Vec4::new(min_point_AABB.x, min_point_AABB.y, min_point_AABB.z, 1.0);
            max_point_AABB = Vec4::new(max_point_AABB.x, max_point_AABB.y, max_point_AABB.z, 1.0);

            let half_extents = (max_point_AABB.xyz() - min_point_AABB.xyz()) / 2.0;
            let center = (max_point_AABB.xyz() + min_point_AABB.xyz()) / 2.0;

            draw.draw_rect_prism(
                half_extents,
                camera_ubo.view_inv * Mat4::from_translation(center),
                Vec3::X,
            );

            break;
        }
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
                debug: false,
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
    let assets = app.resources.get::<Assets>().unwrap();

    // Disable frame rate limit
    app.resources
        .get_mut::<RendererSettings>()
        .unwrap()
        .render_time = None;

    // Load the pipeline
    let pipeline_handle = assets.load::<graphics_assets::Pipeline>(AssetName::new("pbr.pip"));
    assets.wait_for_load(&pipeline_handle);
    let pipeline = assets.get(&pipeline_handle).unwrap().pipeline.clone();

    // Material
    let create_info = MaterialCreateInfo { pipeline };

    let material = factory.create_material(&create_info);

    // Mesh
    let positions = [
        Vec4::new(-0.5, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.5, 0.0, 0.0, 1.0),
        Vec4::new(-0.5, 0.0, 0.0, 1.0),
        Vec4::new(0.5, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
    ];

    let normals = [
        Vec4::new(-1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(-1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
    ];

    let tangents = [
        Vec4::new(-1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(-1.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 1.0, 1.0),
        Vec4::new(0.0, 1.0, 1.0, 1.0),
    ];

    let colors = [
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::new(0.0, 1.0, 0.0, 1.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
    ];

    let indices: Vec<u32> = (0u32..6u32).collect();

    let create_info = MeshCreateInfo {
        positions: &positions,
        normals: Some(&normals),
        tangents: Some(&tangents),
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

    // Create lights
    const LIGHT_COUNT: usize = 128;
    const LIGHT_RING_RADIUS: f32 = 60.0;
    const LIGHT_RANGE: f32 = 16.0;
    const LIGHT_INTENSITY: f32 = 0.25;

    let mut lights = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for i in 0..LIGHT_COUNT {
        let angle = (i as f32 / LIGHT_COUNT as f32) * 2.0 * std::f32::consts::PI;
        let pos = Vec3::new(angle.sin(), 0.0, angle.cos()) * LIGHT_RING_RADIUS;

        lights.0.push(Model(Mat4::from_translation(pos)));
        lights.1.push(PointLight {
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: LIGHT_INTENSITY,
            radius: LIGHT_RANGE,
        });
    }

    app.world.entities().commands().create(lights, &mut []);

    // Register draws
    // NOTE: Unless you want the draw to exist forever, you should store the handles generated here
    // so they can be unregistered later
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();

    const SPACING: f32 = 0.95;
    const WIDTH: usize = 100;
    const DEPTH: usize = 100;
    const HEIGHT: usize = 40;

    let mut bbs = BoundingBoxSystem {
        mesh: mesh.clone(),
        models: [Mat4::IDENTITY; 10],
    };

    let mut i = 0;

    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            for z in 0..DEPTH {
                let position = Vec3::new(
                    x as f32 - (WIDTH as f32 / 2.0),
                    y as f32 - (HEIGHT as f32 / 2.0),
                    z as f32 - (DEPTH as f32 / 2.0),
                ) * SPACING;

                let model = Mat4::from_translation(position);

                if i < 10 {
                    bbs.models[i] = model;
                    i += 1;
                }

                draws.register(
                    &[(
                        Renderable {
                            mesh: mesh.clone(),
                            material: material.clone(),
                        },
                        Model(model),
                    )],
                    &mut [],
                );
            }
        }
    }

    app.dispatcher.add_system(bbs);
}
