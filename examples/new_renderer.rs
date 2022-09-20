use ard_core::prelude::*;
use ard_render::{
    factory::{Factory, ShaderCreateInfo},
    *,
};
use ard_window::prelude::*;
use ard_winit::prelude::*;

fn main() {
    AppBuilder::new(ard_log::LevelFilter::Error)
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
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            debug: true,
        })
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Load in the shaders

    // Create the material

    // Make an instance of the material
}
