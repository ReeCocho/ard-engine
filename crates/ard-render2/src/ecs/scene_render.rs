use ard_ecs::prelude::*;
use ard_math::*;
use ard_render_base::ecs::RenderPreprocessing;
use ard_render_camera::{active::ActiveCamera, Camera, CameraClearColor};
use ard_render_lighting::lights::Lighting;
use ard_render_objects::{Model, RenderFlags};
use ard_render_renderers::{scene::SceneRenderer, shadow::SunShadowsRenderer};

use crate::{canvas::Canvas, factory::Factory, frame::FrameDataRes, FRAMES_IN_FLIGHT};

use super::factory::FactoryProcessingSystem;

#[derive(SystemState, Default)]
pub struct SceneRendererSetup;

const DEFAULT_ACTIVE_CAMERA: ActiveCamera = ActiveCamera {
    camera: Camera {
        near: 0.01,
        far: 100.0,
        fov: 1.571,
        order: 0,
        clear_color: CameraClearColor::None,
        flags: RenderFlags::empty(),
    },
    model: Model(Mat4::IDENTITY),
};

impl SceneRendererSetup {
    #[allow(clippy::type_complexity)]
    fn setup(
        &mut self,
        _: RenderPreprocessing,
        _: Commands,
        _: Queries<()>,
        res: Res<(
            Read<FrameDataRes>,
            Read<Factory>,
            Read<Canvas>,
            Write<SceneRenderer>,
            Write<SunShadowsRenderer>,
            Write<Lighting>,
        )>,
    ) {
        let frame_data = res.get::<FrameDataRes>().unwrap();
        let frame_data = frame_data.inner();
        let factory = res.get::<Factory>().unwrap();
        let canvas = res.get::<Canvas>().unwrap();
        let mut scene_renderer = res.get_mut::<SceneRenderer>().unwrap();
        let mut shadow_renderer = res.get_mut::<SunShadowsRenderer>().unwrap();
        let mut lighting = res.get_mut::<Lighting>().unwrap();

        let meshes = &factory.inner.meshes.lock().unwrap();
        let materials = &factory.inner.materials.lock().unwrap();

        let main_camera = frame_data
            .active_cameras
            .main_camera()
            .unwrap_or(&DEFAULT_ACTIVE_CAMERA);

        let view_location = main_camera.model.position();

        rayon::join(
            || {
                scene_renderer.upload(
                    frame_data.frame,
                    &frame_data.object_data,
                    &meshes,
                    &materials,
                    view_location,
                );
            },
            || {
                shadow_renderer.upload(
                    frame_data.frame,
                    &frame_data.object_data,
                    &meshes,
                    &materials,
                    view_location,
                );
            },
        );

        lighting.update_set(frame_data.frame, &frame_data.lights);

        scene_renderer.update_bindings(
            frame_data.frame,
            &shadow_renderer,
            &lighting,
            &frame_data.object_data,
            &frame_data.lights,
            canvas.hzb(),
            canvas.ao(),
        );

        shadow_renderer.update_bindings::<FRAMES_IN_FLIGHT>(
            frame_data.frame,
            &lighting,
            &frame_data.object_data,
            &frame_data.lights,
        );

        shadow_renderer.update_cascade_views(
            frame_data.frame,
            &main_camera.camera,
            main_camera.model,
            canvas.size(),
            frame_data.lights.global().sun_direction(),
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
