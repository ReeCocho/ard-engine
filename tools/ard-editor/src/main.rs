pub mod camera;
pub mod gui;

use ard_engine::assets::prelude::*;
use ard_engine::core::prelude::*;
use ard_engine::game::GamePlugin;
use ard_engine::render::prelude::PresentMode;
use ard_engine::render::{CanvasSize, Gui, RenderAssetsPlugin, RenderPlugin, RendererSettings};
use ard_engine::window::prelude::*;
use camera::SceneViewCamera;
use gui::EditorView;

fn main() {
    AppBuilder::new(ard_engine::log::LevelFilter::Warn)
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
        .add_plugin(WinitPlugin)
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                present_scene: false,
                render_time: None,
                present_mode: PresentMode::Fifo,
                render_scale: 1.0,
                canvas_size: CanvasSize(Some((512, 512))),
            },
            debug: true,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_plugin(GamePlugin)
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    app.resources.add(SceneViewCamera::new(app));

    let mut gui = app.resources.get_mut::<Gui>().unwrap();
    gui.add_view(EditorView::default());
}
