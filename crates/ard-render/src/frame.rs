use std::time::Duration;

use ard_core::stat::DirtyStaticListener;
use ard_ecs::prelude::*;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::active::ActiveCameras;
use ard_render_gui::GuiRunOutput;
use ard_render_image_effects::{
    ao::AoSettings, sun_shafts2::SunShaftsSettings, tonemapping::TonemappingSettings,
};
use ard_render_lighting::lights::Lights;
use ard_render_objects::objects::RenderObjects;

/// Information used by the render system to draw things. This data is persisted between frames
/// for reuse.
pub struct FrameDataInner {
    // Duration since the last frame.
    pub dt: Duration,
    /// Frame handle for this frame.
    pub frame: Frame,
    /// Listener for dirty static objects.
    pub dirty_static: DirtyStaticListener,
    /// The job of the currently processing frame.
    pub job: Option<Job>,
    /// Gui output to be rendered.
    pub gui_output: GuiRunOutput,
    /// Object data captured from the primary ECS.
    pub object_data: RenderObjects,
    /// Lights captured from the primary ECS.
    pub lights: Lights,
    pub tonemapping_settings: TonemappingSettings,
    pub ao_settings: AoSettings,
    pub sun_shafts_settings: SunShaftsSettings,
    /// Active cameras captured from the primary ECS.
    pub active_cameras: ActiveCameras,
    /// Physical size of the surface window for this frame.
    pub window_size: (u32, u32),
    /// The requested canvas size for this frame.
    pub canvas_size: (u32, u32),
}

pub type FrameData = Box<FrameDataInner>;

#[derive(Resource, Default)]
pub struct FrameDataRes(Option<FrameData>);

impl FrameDataRes {
    #[inline(always)]
    pub fn insert(&mut self, data: FrameData) {
        self.0 = Some(data);
    }

    #[inline(always)]
    pub fn take(&mut self) -> FrameData {
        self.0.take().unwrap()
    }

    #[inline(always)]
    pub fn inner(&self) -> &FrameDataInner {
        self.0.as_ref().unwrap().as_ref()
    }
}
