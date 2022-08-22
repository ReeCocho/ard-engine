pub mod controller;
pub mod editor;
pub mod inspectable;
pub mod scene_graph;
pub mod util;
pub mod view;

use ard_engine::{
    assets::prelude::*, core::prelude::*, game::GamePlugin, graphics::prelude::*,
    window::prelude::*,
};

use ard_engine::graphics_assets::prelude as graphics_assets;

use editor::Editor;
use scene_graph::SceneGraph;
use view::scene_view::SceneViewCamera;

fn main() {
    AppBuilder::new(ard_engine::log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("Ard Editor"),
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
        .add_plugin(GamePlugin)
        .add_plugin(graphics_assets::GraphicsAssetsPlugin)
        .add_resource(SceneGraph::default())
        .add_resource(SceneViewCamera::default())
        .add_startup_function(Editor::setup)
        .run();
}
