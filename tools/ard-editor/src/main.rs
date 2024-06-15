pub mod assets;
pub mod camera;
pub mod gui;
pub mod inspect;
pub mod scene_graph;
pub mod selected;
pub mod tasks;

use ard_engine::assets::prelude::*;
use ard_engine::core::prelude::*;
use ard_engine::game::GamePlugin;
use ard_engine::render::prelude::PresentMode;
use ard_engine::render::{CanvasSize, Gui, RenderAssetsPlugin, RenderPlugin, RendererSettings};
use ard_engine::window::prelude::*;
use assets::importer::AssetImporter;
use assets::EditorAssets;
use camera::SceneViewCamera;
use gui::EditorView;
use scene_graph::{DiscoverSceneGraphRoots, SceneGraph};
use selected::{SelectEntitySystem, Selected};
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
        .add_resource(EditorAssets::new("./assets/").unwrap())
        .add_system(AssetImporter::default())
        .add_system(SelectEntitySystem)
        .add_system(DiscoverSceneGraphRoots)
        .add_startup_function(setup)
        .add_resource(SceneGraph::default())
        .add_resource(Selected::default())
        .run();
}

fn setup(app: &mut App) {
    let (task_runner, task_gui, task_queue) = TaskRunner::new();

    app.dispatcher.add_system(task_runner);

    app.resources.add(SceneViewCamera::new(app));
    app.resources.add(task_queue);

    let mut gui = app.resources.get_mut::<Gui>().unwrap();
    gui.add_view(task_gui);
    gui.add_view(EditorView::default());
}
