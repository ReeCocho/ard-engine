use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::{
    active::{ActiveCamera, ActiveCameras},
    Camera,
};
use ard_render_lighting::{global::GlobalLighting, lights::Lights, Light};
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_render_objects::{objects::RenderObjects, Model, RenderFlags, RenderingMode};
use ard_window::prelude::*;
use crossbeam_channel::{self, Receiver, Sender};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{
    ecs::RenderEcs,
    factory::Factory,
    frame::{FrameData, FrameDataInner},
    RenderPlugin, FRAMES_IN_FLIGHT,
};

#[derive(SystemState)]
pub(crate) struct RenderSystem {
    /// The thread the render ECS is running on.
    thread: Option<std::thread::JoinHandle<()>>,
    /// Used to send messages to the render thread.
    messages: Sender<RenderSystemMessage>,
    /// Queue for frames to process.
    complete_frames: Receiver<FrameData>,
    /// Window ID for the surface.
    surface_window: WindowId,
}

enum RenderSystemMessage {
    Shutdown,
    RenderFrame(FrameData),
}

impl RenderSystem {
    pub fn new<W: HasRawWindowHandle + HasRawDisplayHandle>(
        plugin: RenderPlugin,
        dirty_static: &DirtyStatic,
        window: &W,
        window_id: WindowId,
        window_size: (u32, u32),
    ) -> (Self, Factory) {
        // Channel for render system messages
        let (messages_send, messages_recv) = crossbeam_channel::bounded(FRAMES_IN_FLIGHT + 1);

        // Channel for frames
        let (complete_frames_send, complete_frames_recv) =
            crossbeam_channel::bounded(FRAMES_IN_FLIGHT);

        // Create the render ECS
        let (render_ecs, factory) = RenderEcs::new(plugin, window, window_size);

        // Create default frames
        for frame in 0..FRAMES_IN_FLIGHT {
            complete_frames_send
                .send(Box::new(FrameDataInner {
                    frame: Frame::from(frame),
                    dirty_static: dirty_static.listen().to_all().build(),
                    object_data: RenderObjects::new(render_ecs.ctx().clone()),
                    lights: Lights::new(render_ecs.ctx()),
                    active_cameras: ActiveCameras::default(),
                    job: None,
                    window_size,
                    canvas_size: window_size,
                }))
                .unwrap();
        }

        // Spin up the render thread
        let thread = std::thread::spawn(move || {
            Self::message_pump(render_ecs, messages_recv, complete_frames_send)
        });

        (
            Self {
                thread: Some(thread),
                messages: messages_send,
                complete_frames: complete_frames_recv,
                surface_window: window_id,
            },
            factory,
        )
    }

    /// The render systems `tick` handler is responsible for signaling to the render ECS when
    /// a new frame should be rendered, and additionally preparing all the data that needs to be
    /// sent from the main ECS to the render ECS.
    #[allow(clippy::type_complexity)]
    fn tick(
        &mut self,
        _: Tick,
        _: Commands,
        queries: Queries<(
            Entity,
            (
                Read<Camera>,
                Read<Mesh>,
                Read<Light>,
                Read<MaterialInstance>,
                Read<Model>,
                Read<RenderingMode>,
                Read<RenderFlags>,
                Read<Static>,
            ),
            Read<Disabled>,
        )>,
        res: Res<(Read<Windows>, Read<GlobalLighting>)>,
    ) {
        let windows = res.get::<Windows>().unwrap();
        let global_lighting = res.get::<GlobalLighting>().unwrap();

        // Do not render if the window is minimized
        let window = windows
            .get(self.surface_window)
            .expect("surface window is destroyed");
        if window.physical_height() == 0 || window.physical_width() == 0 {
            return;
        }

        // See if we have a new frame that's ready for rendering
        let mut frame = match self.complete_frames.try_recv() {
            Ok(frame) => frame,
            Err(_) => return,
        };

        // Capture active cameras
        frame.active_cameras.clear();

        for (entity, (camera, model), disabled) in
            queries.make::<(Entity, (Read<Camera>, Read<Model>), Read<Disabled>)>()
        {
            if disabled.is_some() {
                continue;
            }

            frame.active_cameras.insert(
                entity,
                ActiveCamera {
                    camera: camera.clone(),
                    model: *model,
                },
            );
        }

        // Capture render objects

        // Generate queries
        // NOTE: When more functionality is added in the future, it is important to ensure that
        // these queries are mutually exclusive. Otherwise, objects will get rendered twice.
        let static_objs = queries.make::<(
            Entity,
            (
                Read<Mesh>,
                Read<MaterialInstance>,
                Read<Model>,
                Read<RenderingMode>,
                Read<RenderFlags>,
                Read<Static>,
            ),
            Read<Disabled>,
        )>();

        let dynamic_objs = queries.filter().without::<Static>().make::<(
            Entity,
            (
                Read<Mesh>,
                Read<MaterialInstance>,
                Read<Model>,
                Read<RenderingMode>,
                Read<RenderFlags>,
            ),
            Read<Disabled>,
        )>();

        let lights = queries.make::<(Entity, (Read<Light>, Read<Model>), Read<Disabled>)>();

        frame
            .object_data
            .upload_objects(static_objs, dynamic_objs, &frame.dirty_static);

        // Update lighting
        frame.lights.update(lights);
        frame.lights.update_global(&global_lighting);

        // Prepare data for the render thread
        frame.window_size = (window.physical_width(), window.physical_height());
        frame.canvas_size = frame.window_size;

        // Send a message to the render thread to begin rendering the frame
        let _ = self.messages.send(RenderSystemMessage::RenderFrame(frame));
    }

    fn message_pump(
        mut ecs: RenderEcs,
        messages: Receiver<RenderSystemMessage>,
        complete_frames: Sender<FrameData>,
    ) {
        loop {
            match messages.recv() {
                Ok(msg) => match msg {
                    RenderSystemMessage::Shutdown => return,
                    RenderSystemMessage::RenderFrame(mut frame) => {
                        // Wait for the frame to finish rendering.
                        if let Some(job) = frame.job.take() {
                            job.wait_on(None);
                        }

                        // Render the frame
                        let frame = ecs.render(frame);

                        // Put it back into the queue
                        let _ = complete_frames.send(frame);
                    }
                },
                Err(_) => return,
            }
        }
    }
}

impl Drop for RenderSystem {
    fn drop(&mut self) {
        // Send the signal to shutdown and then join the thread
        let _ = self.messages.try_send(RenderSystemMessage::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl From<RenderSystem> for System {
    fn from(state: RenderSystem) -> Self {
        SystemBuilder::new(state)
            .with_handler(RenderSystem::tick)
            .build()
    }
}
