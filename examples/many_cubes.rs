/// When the application loads, press the M key. You should see your mouse cursor disappear. This
/// means the free cam is turned on. You can turn it off by pressing M again. With the free cam on,
/// you should be able to look around with the mouse and move around the scene with WASD.

#[path = "./util.rs"]
mod util;

use ard_assets::{
    prelude::{AssetName, AssetNameBuf, Assets},
    AssetsPlugin,
};
use ard_engine::{core::prelude::*, ecs::prelude::*, math::*, window::prelude::*};

use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{CullMode, FrontFace, PresentMode};
use ard_render::{
    asset::{material::MaterialAsset, RenderAssetsPlugin},
    camera::{CameraClearColor, CameraDescriptor, CameraIbl},
    factory::{Factory, ShaderCreateInfo},
    material::{MaterialCreateInfo, MaterialInstanceCreateInfo},
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    renderer::{gui::Gui, Model, RenderLayer, Renderable, RendererSettings},
    static_geometry::{StaticGeometry, StaticRenderable},
    *,
};

use util::{CameraMover, FrameRate, MainCamera, Settings, StaticHandles, Visualization};

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Error)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1920.0,
                height: 1080.0,
                title: String::from("Many Cubes"),
                vsync: true,
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

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();
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

    // Create the main camera
    let camera_descriptor = CameraDescriptor {
        shadows: None,
        clear_color: CameraClearColor::Color(Vec3::ZERO),
        ibl: CameraIbl {
            diffuse_irradiance: None,
            prefiltered_environment: None,
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
        move_speed: 32.0,
        entity: camera_entity[0],
        position: Vec3::new(0.0, 0.0, -(DEPTH as f32)),
        rotation: Vec3::ZERO,
        descriptor: camera_descriptor,
    });

    // Create cube data
    let vshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/new_render/cube.vert.spv"),
            debug_name: None,
        })
        .unwrap();
    let fshd = factory
        .create_shader(ShaderCreateInfo {
            code: include_bytes!("./assets/new_render/cube.frag.spv"),
            debug_name: None,
        })
        .unwrap();

    let material = factory.create_material(MaterialCreateInfo {
        vertex_shader: vshd,
        depth_only_shader: None,
        fragment_shader: fshd,
        vertex_layout: VertexLayout::COLOR,
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
            colors: Some(&util::CUBE_COLORS),
            uv0: None,
            uv1: None,
            uv2: None,
            uv3: None,
        },
    });

    // Register draws
    // NOTE: Unless you want the draw to exist forever, you should store the handles generated here
    // so they can be unregistered later
    let static_geo = app.resources.get_mut::<StaticGeometry>().unwrap();

    const SPACING: f32 = 0.95;
    const WIDTH: usize = 100;
    const DEPTH: usize = 100;
    const HEIGHT: usize = 100;

    let mut renderables = Vec::with_capacity(WIDTH * HEIGHT * DEPTH);

    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            for z in 0..DEPTH {
                let position = Vec3::new(
                    x as f32 - (WIDTH as f32 / 2.0),
                    y as f32 - (HEIGHT as f32 / 2.0),
                    z as f32 - (DEPTH as f32 / 2.0),
                ) * SPACING;

                let model = Mat4::from_translation(position);

                renderables.push(StaticRenderable {
                    renderable: Renderable {
                        mesh: mesh.clone(),
                        material: material_instance.clone(),
                        layers: RenderLayer::OPAQUE,
                    },
                    model: Model(model),
                    entity: Entity::null(),
                });
            }
        }
    }

    let handles = static_geo.register(&renderables);
    app.resources.add(StaticHandles(handles));
}
