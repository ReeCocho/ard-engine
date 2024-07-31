pub mod assets;
pub mod camera;
pub mod clipboard;
pub mod command;
pub mod gui;
pub mod inspect;
pub mod refresher;
pub mod scene_graph;
pub mod selected;
pub mod ser;
pub mod shlooper;
pub mod tasks;

use ard_engine::assets::prelude::*;
use ard_engine::core::prelude::*;
use ard_engine::game::{GamePlugin, IsEditor};
use ard_engine::physics::PhysicsPlugin;
use ard_engine::render::prelude::PresentMode;
use ard_engine::render::{CanvasSize, Gui, RenderAssetsPlugin, RenderPlugin, RendererSettings};
use ard_engine::transform::TransformPlugin;
use ard_engine::window::prelude::*;
use assets::importer::AssetImporter;
use assets::{AssetManifestLoader, CurrentAssetPath, EditorAssets, EditorAssetsManifest};
use camera::SceneViewCamera;
use clipboard::Clipboard;
use command::{EditorCommandSystem, EditorCommands};
use gui::EditorView;
use refresher::RefresherSystem;
use scene_graph::{DiscoverSceneGraphRoots, SceneGraph};
use selected::{SelectEntitySystem, Selected};
use shlooper::Shlooper;
use tasks::TaskRunner;

fn main() {
    AppBuilder::new(ard_engine::log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Ard Editor"),
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
                present_scene: false,
                render_time: Some(std::time::Duration::from_secs_f32(1.0 / 60.0)),
                present_mode: PresentMode::Mailbox,
                render_scale: 1.0,
                canvas_size: CanvasSize(Some((512, 512))),
            },
            debug: false,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_plugin(GamePlugin)
        .add_resource(IsEditor)
        .add_system(AssetImporter::default())
        .add_system(SelectEntitySystem)
        .add_system(DiscoverSceneGraphRoots)
        .add_system(EditorCommandSystem::default())
        .add_system(Shlooper::default())
        .add_system(RefresherSystem::default())
        .add_resource(SceneGraph::default())
        .add_resource(Selected::default())
        .add_resource(EditorCommands::default())
        .add_resource(CurrentAssetPath::default())
        .add_resource(Clipboard::None)
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let assets = app.resources.get::<Assets>().unwrap().clone();
    assets.register::<EditorAssetsManifest>(AssetManifestLoader);
    app.resources.add(EditorAssets::new(&assets).unwrap());

    let (task_runner, task_gui, task_queue) = TaskRunner::new();

    app.dispatcher.add_system(task_runner);

    app.resources.add(SceneViewCamera::new(app));
    app.resources.add(task_queue);

    let mut gui = app.resources.get_mut::<Gui>().unwrap();
    gui.add_view(task_gui);
    gui.add_view(EditorView::default());
}
