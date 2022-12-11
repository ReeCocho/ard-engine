use ard_engine::{
    assets::prelude::*, core::prelude::*, render::prelude::*,
    window::prelude::*, log::*,
};

fn main() {
    AppBuilder::new(LevelFilter::Warn)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Ard Editor"),
                resizable: true,
                ..Default::default()
            }),
            exit_on_close: true
        })
        .add_plugin(WinitPlugin)
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                present_mode: PresentMode::Mailbox,
                ..Default::default()
            },
            debug: true
        })
        .run();
}
