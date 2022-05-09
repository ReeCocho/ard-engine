use ard_core::prelude::*;
use ard_window::prelude::*;
use ard_winit::prelude::*;

fn main() {
    AppBuilder::new()
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
        .run();
}
