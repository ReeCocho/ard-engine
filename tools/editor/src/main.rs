use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, window::prelude::*,
};

use ard_engine::graphics_assets::prelude as graphics_assets;

#[derive(SystemState)]
struct TestGuiSystem;

impl TestGuiSystem {
    fn pre_render(
        &mut self,
        pre_render: PreRender,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<DebugGui>,)>,
    ) {
        let res = res.get();
        let mut gui = res.0.unwrap();

        let mut opened = true;
        gui.ui().show_demo_window(&mut opened);
    }
}

impl Into<System> for TestGuiSystem {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(TestGuiSystem::pre_render)
            .build()
    }
}

#[tokio::main]
async fn main() {
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
        .add_plugin(graphics_assets::GraphicsAssetsPlugin)
        .add_system(TestGuiSystem)
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    // Disable frame rate limit
    app.resources
        .get_mut::<RendererSettings>()
        .unwrap()
        .render_time = None;
}
