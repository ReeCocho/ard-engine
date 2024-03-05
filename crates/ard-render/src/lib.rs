use std::time::Duration;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_pal::prelude::*;
use ard_render_gui::{Gui, GuiInputCaptureSystem};
use ard_render_lighting::global::GlobalLighting;
use ard_window::prelude::*;
use ard_winit::windows::WinitWindows;
use system::RenderSystem;

pub mod canvas;
pub mod ecs;
pub mod factory;
pub mod frame;
pub mod staging;
pub mod system;

pub const FRAMES_IN_FLIGHT: usize = 2;

#[derive(Clone, Copy)]
pub struct RendererSettings {
    /// Flag to enable drawing the game scene. For games, this should be `true` all the time. This
    /// is useful for things like editors where you only want a GUI.
    pub render_scene: bool,
    /// Time between frame draws. `None` indicates no render limiting.
    pub render_time: Option<Duration>,
    /// Preferred presentation mode.
    pub present_mode: PresentMode,
    /// Type of anti-aliasing to use.
    pub anti_aliasing: AntiAliasingMode,
    /// Super resolution scale factor. A value of `1.0` means no super sampling is performed.
    pub render_scale: f32,
    /// Width and height of the renderer image. `None` indicates the dimensions should match that
    /// of the surface being presented to.
    pub canvas_size: Option<(u32, u32)>,
}

#[derive(Clone, Copy)]
pub enum AntiAliasingMode {
    None,
    MSAA(MultiSamples),
}

#[derive(Resource, Clone)]
pub struct RenderPlugin {
    pub window: WindowId,
    pub settings: RendererSettings,
    pub debug: bool,
}

impl Plugin for RenderPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(self.clone());
        app.add_resource(GlobalLighting::default());
        app.add_resource(Gui::default());
        app.add_system(GuiInputCaptureSystem);
        app.add_startup_function(late_render_init);
    }
}

fn late_render_init(app: &mut App) {
    let plugin = app.resources.get::<RenderPlugin>().unwrap().clone();
    let windows = app.resources.get::<WinitWindows>().unwrap();
    let dirty_static = app.resources.get::<DirtyStatic>().unwrap();
    let window = windows.get_window(plugin.window).unwrap();
    let size = window.inner_size();

    let window_id = plugin.window;
    let (render_system, factory) = RenderSystem::new(
        plugin,
        &dirty_static,
        window,
        window_id,
        (size.width, size.height),
    );

    app.dispatcher.add_system(render_system);
    app.resources.add(factory);
}

impl From<AntiAliasingMode> for MultiSamples {
    fn from(value: AntiAliasingMode) -> Self {
        match value {
            AntiAliasingMode::MSAA(samples) => samples,
            _ => MultiSamples::Count1,
        }
    }
}
