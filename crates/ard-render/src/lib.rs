pub mod asset;
pub mod camera;
pub mod cube_map;
pub mod factory;
pub mod lighting;
pub mod material;
pub mod mesh;
pub mod pbr;
pub mod renderer;
pub mod shader_constants;
pub mod static_geometry;
pub mod texture;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_window::window::WindowId;
use ard_winit::windows::WinitWindows;
use renderer::{Renderer, RendererSettings};

#[derive(Clone)]
pub struct RenderPlugin {
    pub window: WindowId,
    pub settings: RendererSettings,
    pub debug: bool,
}

#[derive(Resource, Clone)]
struct LateRenderInit(RenderPlugin);

impl Plugin for RenderPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(LateRenderInit(self.clone()));
        app.add_startup_function(late_render_init);
    }
}

fn late_render_init(app: &mut App) {
    let plugin = app.resources.get::<LateRenderInit>().unwrap().clone();
    let windows = app.resources.get::<WinitWindows>().unwrap();
    let window = windows.get_window(plugin.0.window).unwrap();
    let size = window.inner_size();

    let (renderer, factory, static_geo, lighting, gui) = Renderer::new(
        plugin.0.clone(),
        window,
        plugin.0.window,
        (size.width, size.height),
        &plugin.0.settings,
    );

    app.dispatcher.add_system(renderer);
    app.resources.add(gui);
    app.resources.add(lighting);
    app.resources.add(plugin.0.settings);
    app.resources.add(factory);
    app.resources.add(static_geo);
}
