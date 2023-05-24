// pub mod asset_import;
pub mod drag_drog;
pub mod editor;
pub mod inspection;
pub mod meta;
pub mod util;
pub mod views;

use ard_engine::{
    assets::prelude::*, core::prelude::*, game::GamePlugin, log::*, render::prelude::*,
    window::prelude::*,
};
// use asset_import::AssetImportPlugin;
use editor::EditorGuiView;
use views::{scene::SceneView, EditorDockTree, EditorPanels};

fn main() {
    AppBuilder::new(LevelFilter::Warn)
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
                present_mode: PresentMode::Mailbox,
                ..Default::default()
            },
            debug: false,
        })
        .add_plugin(RenderAssetsPlugin {
            pbr_material: AssetNameBuf::from("materials/pbr.mat"),
        })
        .add_plugin(GamePlugin)
        // .add_plugin(AssetImportPlugin)
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let mut gui = app.resources.get_mut::<Gui>().unwrap();
    let mut settings = app.resources.get_mut::<RendererSettings>().unwrap();
    settings.render_time = None;

    // Construct editor
    app.resources.add(EditorDockTree::default());
    app.resources.add(EditorPanels {
        panel_SceneView: SceneView::default(),
    });
    gui.add_view(EditorGuiView);
}
