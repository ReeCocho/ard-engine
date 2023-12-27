use ard_ecs::prelude::*;
use ard_math::*;
use ard_render_base::ecs::RenderPreprocessing;
use ard_render_lighting::lights::Lighting;
use ard_render_renderers::scene::SceneRenderer;

use crate::{canvas::Canvas, factory::Factory, frame::FrameDataRes};

use super::factory::FactoryProcessingSystem;

#[derive(SystemState, Default)]
pub struct SceneRendererSetup;

impl SceneRendererSetup {
    #[allow(clippy::type_complexity)]
    fn setup(
        &mut self,
        _: RenderPreprocessing,
        _: Commands,
        _: Queries<()>,
        res: Res<(
            Read<FrameDataRes>,
            Read<Canvas>,
            Read<Factory>,
            Write<SceneRenderer>,
            Write<Lighting>,
        )>,
    ) {
        let frame_data = res.get::<FrameDataRes>().unwrap();
        let canvas = res.get::<Canvas>().unwrap();
        let frame_data = frame_data.inner();
        let factory = res.get::<Factory>().unwrap();
        let mut scene_renderer = res.get_mut::<SceneRenderer>().unwrap();
        let mut lighting = res.get_mut::<Lighting>().unwrap();

        scene_renderer.upload(
            frame_data.frame,
            &frame_data.object_data,
            &factory.inner.meshes.lock().unwrap(),
            &factory.inner.materials.lock().unwrap(),
            frame_data
                .active_cameras
                .main_camera()
                .map_or(Vec3A::ZERO, |camera| camera.model.position()),
        );

        lighting.update_set(frame_data.frame, &frame_data.lights);

        scene_renderer.update_bindings(
            frame_data.frame,
            &lighting,
            &frame_data.object_data,
            &frame_data.lights,
            canvas.hzb(),
        );
    }
}

impl From<SceneRendererSetup> for System {
    fn from(value: SceneRendererSetup) -> Self {
        SystemBuilder::new(value)
            .with_handler(SceneRendererSetup::setup)
            .run_after::<RenderPreprocessing, FactoryProcessingSystem>()
            .build()
    }
}
