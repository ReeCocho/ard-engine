use ard_engine::assets::prelude::*;
use ard_engine::core::prelude::*;
use ard_engine::game::save_data::{InitialSceneAsset, SceneAsset, INITIAL_SCENE_ASSET_NAME};
use ard_engine::game::settings::GameSettings;
use ard_engine::game::{GamePlugin, GameStart};
use ard_engine::physics::PhysicsPlugin;
use ard_engine::render::prelude::PresentMode;
use ard_engine::render::{CanvasSize, RenderAssetsPlugin, RenderPlugin, RendererSettings};
use ard_engine::save_load::format::Ron;
use ard_engine::transform::TransformPlugin;
use ard_engine::window::prelude::*;

fn main() {
    AppBuilder::new(ard_engine::log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Ard Game"),
                resizable: true,
                width: 1280.0,
                height: 720.0,
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(TransformPlugin)
        .add_plugin(PhysicsPlugin)
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                present_scene: true,
                render_time: None,
                present_mode: PresentMode::Mailbox,
                render_scale: 1.0,
                canvas_size: CanvasSize(None),
            },
            debug: true,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_plugin(GamePlugin)
        .add_resource(GameSettings::load().unwrap_or_default())
        .add_startup_function(load_initial_scene)
        .run();
}

fn load_initial_scene(app: &mut App) {
    let assets = app.resources.get::<Assets>().unwrap().clone();
    let handle = assets
        .load::<InitialSceneAsset>(AssetName::new(INITIAL_SCENE_ASSET_NAME))
        .unwrap();
    assets.wait_for_load(&handle);

    let asset = assets.get(&handle).unwrap();
    let handle = assets.load::<SceneAsset>(&asset.asset_name).unwrap();
    assets.wait_for_load(&handle);

    let asset = assets.get(&handle).unwrap();
    SceneAsset::loader::<Ron>()
        .load(
            asset.data().clone(),
            assets.clone(),
            app.world.entities().commands(),
        )
        .unwrap();

    app.dispatcher.submit(GameStart);
}
