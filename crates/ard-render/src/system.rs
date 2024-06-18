use std::time::{Duration, Instant};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render_base::{ecs::Frame, RenderingMode, FRAMES_IN_FLIGHT};
use ard_render_camera::{
    active::{ActiveCamera, ActiveCameras},
    Camera,
};
use ard_render_debug::{buffer::DebugVertexBuffer, DebugDrawing};
use ard_render_gui::{Gui, GuiRunOutput};
use ard_render_image_effects::{
    ao::AoSettings, smaa::SmaaSettings, sun_shafts2::SunShaftsSettings,
    tonemapping::TonemappingSettings,
};
use ard_render_lighting::{global::GlobalLighting, lights::Lights, Light};
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_render_objects::{objects::RenderObjects, Model, RenderFlags};
use ard_render_renderers::{entities::SelectEntity, pathtracer::PathTracerSettings};
use ard_window::prelude::*;
use crossbeam_channel::{self, Receiver, Sender};
use raw_window_handle::HasDisplayHandle;

use crate::{
    ecs::RenderEcs,
    factory::Factory,
    frame::{FrameData, FrameDataInner, WindowInfo},
    CanvasSize, DebugSettings, MsaaSettings, PresentationSettings, RenderPlugin,
};

#[derive(SystemState)]
pub struct RenderSystem {
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
    // Frame rate cap.
    render_time: Option<Duration>,
    // Pending request to select an entity.
    select_entity: Option<SelectEntity>,
}

#[derive(Debug, Event, Copy, Clone)]
pub struct PreRender;

enum RenderSystemMessage {
    Shutdown,
    RenderFrame(FrameData),
}

impl RenderSystem {
    pub fn new<D: HasDisplayHandle>(
        plugin: RenderPlugin,
        dirty_static: &DirtyStatic,
        display_handle: &D,
    ) -> (Self, Factory) {
        // Channel for render system messages
        let (messages_send, messages_recv) = crossbeam_channel::bounded(FRAMES_IN_FLIGHT + 1);

        // Channel for frames
        let (complete_frames_send, complete_frames_recv) =
            crossbeam_channel::bounded(FRAMES_IN_FLIGHT);

        // Create the render ECS
        let render_time = plugin.settings.render_time;
        let present_scene = plugin.settings.present_scene;
        let present_mode = plugin.settings.present_mode;
        let window_id = plugin.window;
        let (render_ecs, factory) = RenderEcs::new(plugin, display_handle);

        // Create default frames
        for frame in 0..FRAMES_IN_FLIGHT {
            complete_frames_send
                .send(Box::new(FrameDataInner {
                    frame: Frame::from(frame),
                    present_scene,
                    dt: Duration::from_millis(1),
                    dirty_static: dirty_static.listen().to_all().build(),
                    gui_output: GuiRunOutput::default(),
                    object_data: RenderObjects::new(render_ecs.ctx().clone()),
                    lights: Lights::new(render_ecs.ctx()),
                    debug_vertices: DebugVertexBuffer::new(render_ecs.ctx()),
                    present_settings: PresentationSettings { present_mode },
                    debug_settings: DebugSettings::default(),
                    tonemapping_settings: TonemappingSettings::default(),
                    ao_settings: AoSettings::default(),
                    sun_shafts_settings: SunShaftsSettings::default(),
                    smaa_settings: SmaaSettings::default(),
                    msaa_settings: MsaaSettings::default(),
                    path_tracer_settings: PathTracerSettings::default(),
                    active_cameras: ActiveCameras::default(),
                    select_entity: None,
                    selected_entity: None,
                    job: None,
                    window: None,
                    canvas_size: (16, 16),
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
                render_time,
                select_entity: None,
            },
            factory,
        )
    }

    fn select_entity(&mut self, evt: SelectEntity, _: Commands, _: Queries<()>, _: Res<()>) {
        self.select_entity = Some(evt);
    }

    /// The render systems `tick` handler is responsible for signaling to the render ECS when
    /// a new frame should be rendered, and additionally preparing all the data that needs to be
    /// sent from the main ECS to the render ECS.
    fn tick(&mut self, _: Tick, commands: Commands, _: Queries<()>, res: Res<(Read<Windows>,)>) {
        // Do not render if enough time has not passed
        if let Some(rate) = self.render_time {
            if Instant::now().duration_since(self.last_frame_time) < rate {
                return;
            }
        }

        let windows = res.get::<Windows>().unwrap();

        // Do not render if the window is minimized or not created yet
        let window = match windows.get(self.surface_window) {
            Some(window) => window,
            None => return,
        };
        let physical_width = window.physical_width();
        let physical_height = window.physical_height();
        if physical_width == 0 || physical_height == 0 {
            return;
        }

        std::mem::drop(windows);

        // See if we have a new frame that's ready for rendering
        if self.complete_frames.is_empty() {
            return;
        }

        // Send a message to all other systems that rendering just finished
        commands.events.submit(PreRender);
    }

    /// This pre render command prepares all the data for rendering and submits the request.
    fn pre_render(
        &mut self,
        _: PreRender,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<Everything>,
    ) {
        let windows = res.get::<Windows>().unwrap();
        let mut debug_draw = res.get_mut::<DebugDrawing>().unwrap();

        // Do not render if the window is minimized or not created yet
        let window = match windows.get(self.surface_window) {
            Some(window) => window,
            None => {
                debug_draw.clear();
                return;
            }
        };
        let physical_width = window.physical_width();
        let physical_height = window.physical_height();
        if physical_width == 0 || physical_height == 0 {
            debug_draw.clear();
            return;
        }

        let window_handle = window.window_handle();
        let display_handle = window.display_handle();

        std::mem::drop(windows);
        std::mem::drop(debug_draw);

        let mut frame = self.complete_frames.recv().unwrap();

        // If an entity was selected, send the event
        if let Some(evt) = frame.selected_entity.take() {
            commands.events.submit(evt);
        }

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

        frame.object_data.upload_objects(
            frame.frame,
            static_objs,
            dynamic_objs,
            &frame.dirty_static,
        );

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

        // Capture debugging draws.
        let mut debug_draws = res.get_mut::<DebugDrawing>().unwrap();
        frame.debug_vertices.write_draws(debug_draws.draws());
        debug_draws.clear();

        // Set cursor icon
        let mut windows = res.get_mut::<Windows>().unwrap();
        let window = windows.get_mut(self.surface_window).unwrap();
        window.set_cursor_icon(match frame.gui_output.full.platform_output.cursor_icon {
            egui::CursorIcon::Default => CursorIcon::Default,
            egui::CursorIcon::None => CursorIcon::Default,
            egui::CursorIcon::ContextMenu => CursorIcon::ContextMenu,
            egui::CursorIcon::Help => CursorIcon::Help,
            egui::CursorIcon::PointingHand => CursorIcon::Pointer,
            egui::CursorIcon::Progress => CursorIcon::Progress,
            egui::CursorIcon::Wait => CursorIcon::Wait,
            egui::CursorIcon::Cell => CursorIcon::Cell,
            egui::CursorIcon::Crosshair => CursorIcon::Crosshair,
            egui::CursorIcon::Text => CursorIcon::Text,
            egui::CursorIcon::VerticalText => CursorIcon::VerticalText,
            egui::CursorIcon::Alias => CursorIcon::Alias,
            egui::CursorIcon::Copy => CursorIcon::Copy,
            egui::CursorIcon::Move => CursorIcon::Move,
            egui::CursorIcon::NoDrop => CursorIcon::NoDrop,
            egui::CursorIcon::NotAllowed => CursorIcon::NotAllowed,
            egui::CursorIcon::Grab => CursorIcon::Grab,
            egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
            egui::CursorIcon::AllScroll => CursorIcon::AllScroll,
            egui::CursorIcon::ResizeHorizontal => CursorIcon::EwResize,
            egui::CursorIcon::ResizeNeSw => CursorIcon::NeswResize,
            egui::CursorIcon::ResizeNwSe => CursorIcon::NwseResize,
            egui::CursorIcon::ResizeVertical => CursorIcon::NsResize,
            egui::CursorIcon::ResizeEast => CursorIcon::EResize,
            egui::CursorIcon::ResizeSouthEast => CursorIcon::SeResize,
            egui::CursorIcon::ResizeSouth => CursorIcon::SResize,
            egui::CursorIcon::ResizeSouthWest => CursorIcon::SwResize,
            egui::CursorIcon::ResizeWest => CursorIcon::WResize,
            egui::CursorIcon::ResizeNorthWest => CursorIcon::NwResize,
            egui::CursorIcon::ResizeNorth => CursorIcon::NResize,
            egui::CursorIcon::ResizeNorthEast => CursorIcon::NeResize,
            egui::CursorIcon::ResizeColumn => CursorIcon::ColResize,
            egui::CursorIcon::ResizeRow => CursorIcon::RowResize,
            egui::CursorIcon::ZoomIn => CursorIcon::ZoomIn,
            egui::CursorIcon::ZoomOut => CursorIcon::ZoomOut,
        });

        // Prepare data for the render thread
        frame.window = Some(WindowInfo {
            size: (physical_width, physical_height),
            window_handle,
            display_handle,
        });
        frame.canvas_size = res
            .get::<CanvasSize>()
            .unwrap()
            .0
            .unwrap_or((physical_width, physical_height));
        frame.dt = dt;
        frame.tonemapping_settings = *res.get::<TonemappingSettings>().unwrap();
        frame.ao_settings = *res.get::<AoSettings>().unwrap();
        frame.sun_shafts_settings = *res.get::<SunShaftsSettings>().unwrap();
        frame.smaa_settings = *res.get::<SmaaSettings>().unwrap();
        frame.msaa_settings = *res.get::<MsaaSettings>().unwrap();
        frame.debug_settings = *res.get::<DebugSettings>().unwrap();
        frame.path_tracer_settings = *res.get::<PathTracerSettings>().unwrap();
        frame.select_entity = self.select_entity.take();

        self.last_frame_time = now;

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
            .with_handler(RenderSystem::pre_render)
            .with_handler(RenderSystem::select_entity)
            .build()
    }
}
