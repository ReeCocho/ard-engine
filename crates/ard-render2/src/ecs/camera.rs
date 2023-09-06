use ard_ecs::prelude::*;
use ard_render_base::ecs::RenderPreprocessing;
use ard_render_camera::ubo::CameraUbo;

use crate::{canvas::Canvas, frame::FrameDataRes};

#[derive(SystemState, Default)]
pub struct CameraSetupSystem;

impl CameraSetupSystem {
    fn setup(
        &mut self,
        _: RenderPreprocessing,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<FrameDataRes>, Read<Canvas>, Write<CameraUbo>)>,
    ) {
        let frame_data = res.get::<FrameDataRes>().unwrap();
        let frame_data = frame_data.inner();
        let canvas = res.get::<Canvas>().unwrap();
        let mut ubo = res.get_mut::<CameraUbo>().unwrap();

        if let Some(camera) = frame_data.active_cameras.main_camera() {
            // Update the UBO
            let (width, height) = canvas.size();
            ubo.update(
                frame_data.frame,
                &camera.camera,
                width,
                height,
                camera.model,
            );
        }
    }
}

impl From<CameraSetupSystem> for System {
    fn from(value: CameraSetupSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(CameraSetupSystem::setup)
            .build()
    }
}
