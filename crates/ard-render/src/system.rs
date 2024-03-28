use std::time::{Duration, Instant};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render_base::{ecs::Frame, RenderingMode};
use ard_render_camera::{
    active::{ActiveCamera, ActiveCameras},
    Camera,
};
use ard_render_gui::{Gui, GuiInputCaptureSystem, GuiRunOutput};
use ard_render_image_effects::{
    ao::AoSettings, smaa::SmaaSettings, sun_shafts2::SunShaftsSettings,
    tonemapping::TonemappingSettings,
};
use ard_render_lighting::{global::GlobalLighting, lights::Lights, Light};
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_render_objects::{objects::RenderObjects, Model, RenderFlags};
use ard_window::prelude::*;
use crossbeam_channel::{self, Receiver, Sender};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{
    ecs::RenderEcs,
    factory::Factory,
    frame::{FrameData, FrameDataInner},
    DebugSettings, MsaaSettings, RenderPlugin, FRAMES_IN_FLIGHT,
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
    // Instant the last frame was sent.
    last_frame_time: Instant,
}

#[derive(Debug, Event, Copy, Clone)]
pub struct PostRender;

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
                    dt: Duration::from_millis(1),
                    dirty_static: dirty_static.listen().to_all().build(),
                    gui_output: GuiRunOutput::default(),
                    object_data: RenderObjects::new(render_ecs.ctx().clone()),
                    lights: Lights::new(render_ecs.ctx()),
                    debug_settings: DebugSettings::default(),
                    tonemapping_settings: TonemappingSettings::default(),
                    ao_settings: AoSettings::default(),
                    sun_shafts_settings: SunShaftsSettings::default(),
                    smaa_settings: SmaaSettings::default(),
                    msaa_settings: MsaaSettings::default(),
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
                last_frame_time: Instant::now(),
            },
            factory,
        )
    }

    /// The render systems `tick` handler is responsible for signaling to the render ECS when
    /// a new frame should be rendered, and additionally preparing all the data that needs to be
    /// sent from the main ECS to the render ECS.
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<Everything>,
    ) {
        let windows = res.get::<Windows>().unwrap();

        // Do not render if the window is minimized
        let window = windows
            .get(self.surface_window)
            .expect("surface window is destroyed");
        let physical_width = window.physical_width();
        let physical_height = window.physical_height();
        if physical_width == 0 || physical_height == 0 {
            return;
        }
        std::mem::drop(windows);

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
        let global_lighting = res.get::<GlobalLighting>().unwrap();
        frame.lights.update(lights);
        frame.lights.update_global(&global_lighting);
        std::mem::drop(global_lighting);

        // Render GUI
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time);
        let mut gui = res.get_mut::<Gui>().unwrap();
        frame.gui_output = gui.run(Tick(dt), &commands, &queries, &res);

        // Prepare data for the render thread
        frame.window_size = (physical_width, physical_height);
        frame.canvas_size = frame.window_size;
        frame.dt = dt;
        frame.tonemapping_settings = *res.get::<TonemappingSettings>().unwrap();
        frame.ao_settings = *res.get::<AoSettings>().unwrap();
        frame.sun_shafts_settings = *res.get::<SunShaftsSettings>().unwrap();
        frame.smaa_settings = *res.get::<SmaaSettings>().unwrap();
        frame.msaa_settings = *res.get::<MsaaSettings>().unwrap();
        frame.debug_settings = *res.get::<DebugSettings>().unwrap();

        self.last_frame_time = now;

        // Send a message to the render thread to begin rendering the frame
        let _ = self.messages.send(RenderSystemMessage::RenderFrame(frame));

        // Send a message to all other systems that rendering just finished
        commands.events.submit(PostRender);
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
            .run_after::<Tick, GuiInputCaptureSystem>()
            .build()
    }
}
