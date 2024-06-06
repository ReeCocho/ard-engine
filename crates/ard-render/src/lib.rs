use std::time::Duration;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_pal::prelude::*;
use ard_render_gui::{Gui, GuiInputCaptureSystem};
use ard_render_image_effects::{
    ao::AoSettings, smaa::SmaaSettings, sun_shafts2::SunShaftsSettings,
    tonemapping::TonemappingSettings,
};
use ard_render_lighting::global::GlobalLighting;
use ard_render_renderers::pathtracer::PathTracerSettings;
use ard_window::prelude::*;
use system::RenderSystem;

pub mod blas;
pub mod canvas;
pub mod ecs;
pub mod factory;
pub mod frame;
pub mod staging;
pub mod system;

#[derive(Clone, Copy)]
pub struct RendererSettings {
    /// Flag to enable drawing the game scene to the screen. For games, this should be `true` all
    /// the time. This is useful for things like editors where you only want a GUI.
    pub present_scene: bool,
    /// Time between frame draws. `None` indicates no render limiting.
    pub render_time: Option<Duration>,
    /// Preferred presentation mode.
    pub present_mode: PresentMode,
    /// Super resolution scale factor. A value of `1.0` means no super sampling is performed.
    pub render_scale: f32,
    /// Width and height of the renderer image. `None` indicates the dimensions should match that
    /// of the surface being presented to.
    pub canvas_size: CanvasSize,
}
/// Width and height of the renderer image. `None` indicates the dimensions should match that
/// of the surface being presented to.
#[derive(Resource, Default, Clone, Copy)]
pub struct CanvasSize(pub Option<(u32, u32)>);

#[derive(Resource, Default, Clone, Copy)]
pub struct PresentationSettings {
    pub present_mode: PresentMode,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct DebugSettings {
    pub lock_culling: bool,
}

#[derive(Resource, Clone, Copy)]
pub struct MsaaSettings {
    pub samples: MultiSamples,
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
        app.add_resource(TonemappingSettings::default());
        app.add_resource(AoSettings::default());
        app.add_resource(SunShaftsSettings::default());
        app.add_resource(SmaaSettings::default());
        app.add_resource(MsaaSettings::default());
        app.add_resource(DebugSettings::default());
        app.add_resource(PathTracerSettings::default());
        app.add_resource(Gui::default());
        app.add_system(GuiInputCaptureSystem);
        app.add_startup_function(late_render_init);
    }
}

impl Default for MsaaSettings {
    fn default() -> Self {
        MsaaSettings {
            samples: MultiSamples::Count1,
        }
    }
}

fn late_render_init(app: &mut App) {
    let plugin = app.resources.get::<RenderPlugin>().unwrap().clone();
    let windows = app.resources.get::<Windows>().unwrap();
    let dirty_static = app.resources.get::<DirtyStatic>().unwrap();

    app.resources.add(PresentationSettings {
        present_mode: plugin.settings.present_mode,
    });
    app.resources.add(plugin.settings.canvas_size);

    let (render_system, factory) =
        RenderSystem::new(plugin, &dirty_static, windows.display_handle());

    app.dispatcher.add_system(render_system);
    app.resources.add(factory);
}
