pub mod factory;
pub mod material;
pub mod mesh;
pub mod renderer;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_window::window::WindowId;
use ard_winit::windows::WinitWindows;
use renderer::{Renderer, RendererSettings};

#[derive(Copy, Clone)]
pub struct RenderPlugin {
    pub window: WindowId,
    pub debug: bool,
}

#[derive(Resource, Clone, Copy)]
struct LateRenderInit(RenderPlugin);

impl Plugin for RenderPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(LateRenderInit(*self));
        app.add_startup_function(late_render_init);
    }
}

fn late_render_init(app: &mut App) {
    let plugin = *app.resources.get::<LateRenderInit>().unwrap();
    let windows = app.resources.get::<WinitWindows>().unwrap();
    let window = windows.get_window(plugin.0.window).unwrap();
    let size = window.inner_size();
    let renderer_settings = RendererSettings::default();

    let (renderer, factory) = Renderer::new(
        plugin.0,
        window,
        plugin.0.window,
        (size.width, size.height),
        &renderer_settings,
    );

    app.dispatcher.add_system(renderer);
    app.resources.add(renderer_settings);
    app.resources.add(factory);
}
