use ard_ecs::prelude::*;
use ard_math::*;
use ard_render_base::ecs::RenderPreprocessing;
use ard_render_renderers::scene::SceneRenderer;

use crate::{factory::Factory, frame::FrameDataRes};

use super::factory::FactoryProcessingSystem;

#[derive(SystemState, Default)]
pub struct SceneRendererSetup;

impl SceneRendererSetup {
    fn setup(
        &mut self,
        _: RenderPreprocessing,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<FrameDataRes>, Read<Factory>, Write<SceneRenderer>)>,
    ) {
        let frame_data = res.get::<FrameDataRes>().unwrap();
        let frame_data = frame_data.inner();
        let factory = res.get::<Factory>().unwrap();
        let mut scene_renderer = res.get_mut::<SceneRenderer>().unwrap();

        scene_renderer.upload(
            frame_data.frame,
            &frame_data.object_data,
            &mut factory.inner.meshes.lock().unwrap(),
            Vec3A::ZERO,
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
