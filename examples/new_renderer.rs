use ard_assets::prelude::*;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_formats::mesh::VertexLayout;
use ard_math::{Mat4, Vec3};
use ard_pal::prelude::{CullMode, FrontFace, PresentMode};
use ard_render::{
    asset::{
        cube_map::CubeMapAsset, material::MaterialAsset, model::ModelAsset, RenderAssetsPlugin,
    },
    camera::{CameraClearColor, CameraDescriptor, CameraIbl, CameraShadows},
    factory::{Factory, ShaderCreateInfo},
    lighting::PointLight,
    material::{MaterialCreateInfo, MaterialInstanceCreateInfo},
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    renderer::{gui::Gui, Model, RenderLayer, Renderable, RendererSettings},
    static_geometry::StaticGeometry,
    *,
};
use ard_window::prelude::*;
use ard_winit::prelude::*;
use rand::Rng;

use crate::util::{CameraMover, FrameRate, MainCamera, Settings, StaticHandles, Visualization};

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
                present_mode: PresentMode::Mailbox,
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

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();
    let static_geo = app.resources.get::<StaticGeometry>().unwrap();
    let mut gui = app.resources.get_mut::<Gui>().unwrap();

    // Disable frame rate limit
    let mut settings = app.resources.get_mut::<RendererSettings>().unwrap();
    // settings.render_time = None;
    settings.render_scale = 1.0;

    // Add in GUI views
    let slice_view_handle = assets.load::<MaterialAsset>(AssetName::new("slice_vis.mat"));
    let cascade_view_handle = assets.load::<MaterialAsset>(AssetName::new("cascade_vis.mat"));
    let cluster_heatmap_handle =
        assets.load::<MaterialAsset>(AssetName::new("cluster_heatmap.mat"));
    assets.wait_for_load(&slice_view_handle);
    assets.wait_for_load(&cascade_view_handle);
    assets.wait_for_load(&cluster_heatmap_handle);

    gui.add_view(Settings {
        visualization: Visualization::None,
        slice_view_mat: assets.get(&slice_view_handle).unwrap().material.clone(),
        cascade_view_mat: assets.get(&cascade_view_handle).unwrap().material.clone(),
        cluster_heatmap_mat: assets
            .get(&cluster_heatmap_handle)
            .unwrap()
            .material
            .clone(),
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
