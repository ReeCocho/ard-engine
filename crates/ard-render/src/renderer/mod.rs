pub mod render_data;

use std::time::{Duration, Instant};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_log::warn;
use ard_math::Mat4;
use ard_pal::prelude::*;
use ard_window::{
    window::{Window, WindowId},
    windows::Windows,
};
use bitflags::bitflags;
use raw_window_handle::HasRawWindowHandle;

use crate::{
    factory::{allocator::ResourceId, Factory},
    material::{MaterialInstance, PipelineType},
    mesh::Mesh,
    shader_constants::FRAMES_IN_FLIGHT,
    RenderPlugin,
};

use self::render_data::{GlobalRenderData, RenderArgs};

#[derive(Resource)]
pub struct RendererSettings {
    /// Flag to enable drawing the game scene. For games, this should be `true` all the time. This
    /// is useful for things like editors where you only want a GUI.
    pub render_scene: bool,
    /// Time between frame draws. `None` indicates no render limiting.
    pub render_time: Option<Duration>,
    /// Preferred presentation mode.
    pub present_mode: PresentMode,
    /// Width and height of the renderer image. `None` indicates the dimensions should match that
    /// of the surface being presented to.
    pub canvas_size: Option<(u32, u32)>,
    /// Anisotropy level to be used for textures. Can be `None` for no filtering.
    pub anisotropy_level: Option<AnisotropyLevel>,
}

#[derive(SystemState)]
pub struct Renderer {
    surface_window: WindowId,
    surface: Surface,
    surface_format: TextureFormat,
    present_mode: PresentMode,
    last_render_time: Instant,
    frame: usize,
    global_data: GlobalRenderData,
    depth_buffer: Texture,
    final_image: Texture,
    ctx: Context,
}

/// The geometry and material of an object to render.
#[derive(Component)]
pub struct Renderable {
    pub mesh: Mesh,
    pub material: MaterialInstance,
    pub layers: RenderLayer,
}

/// Model matrix of a renderable object.
#[derive(Component)]
pub struct Model(pub Mat4);

bitflags! {
    /// Layer mask used for renderable objects.
    pub struct RenderLayer: u8 {
        const OPAQUE        = 0b0000_0001;
        const SHADOW_CASTER = 0b0000_0010;
    }
}

/// Event indicating that rendering is about to be performed. Contains the duration sine the
/// last pre render event.
#[derive(Debug, Event, Copy, Clone)]
pub struct PreRender(pub Duration);

/// Internal render event.
#[derive(Debug, Event, Copy, Clone)]
struct Render(Duration);

/// Event indicating that rendering has finished. Contains the duration since the
/// last post render event.
#[derive(Debug, Event, Copy, Clone)]
pub struct PostRender(pub Duration);

pub(crate) type RenderQuery = (Entity, (Read<Renderable>, Read<Model>), (Read<Disabled>,));
type RenderResources = (Read<RendererSettings>, Read<Windows>, Write<Factory>);

impl Renderer {
    pub fn new<W: HasRawWindowHandle>(
        plugin: RenderPlugin,
        window: &W,
        window_id: WindowId,
        window_size: (u32, u32),
        settings: &RendererSettings,
    ) -> (Self, Factory) {
        // Initialize the backend based on what renderer we want
        #[cfg(feature = "vulkan")]
        let backend = {
            ard_pal::backend::VulkanBackend::new(ard_pal::backend::VulkanBackendCreateInfo {
                app_name: String::from("ard"),
                engine_name: String::from("ard"),
                window,
                debug: plugin.debug,
            })
            .unwrap()
        };

        #[cfg(not(any(feature = "vulkan")))]
        let backend = {
            panic!("no rendering backend selected");
            ()
        };

        // Create our graphics context
        let ctx = Context::new(backend);

        // Create our surface
        let surface = Surface::new(
            ctx.clone(),
            SurfaceCreateInfo {
                config: SurfaceConfiguration {
                    width: window_size.0,
                    height: window_size.1,
                    present_mode: settings.present_mode,
                    format: TextureFormat::Bgra8Unorm,
                },
                window,
                debug_name: Some(String::from("primary_surface")),
            },
        )
        .unwrap();

        // Create the final texture to copy to the swapchain image
        let (width, height) = match settings.canvas_size {
            Some(dims) => dims,
            None => window_size,
        };
        let final_image = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Bgra8Unorm,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("final_image")),
            },
        )
        .unwrap();
        let depth_buffer = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::D32Sfloat,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("depth_buffer")),
            },
        )
        .unwrap();

        // Create render data
        let global_data = GlobalRenderData::new(&ctx);

        // Create the factory
        let factory = Factory::new(ctx.clone(), settings.anisotropy_level, &global_data);

        (
            Self {
                ctx,
                surface_window: window_id,
                surface,
                surface_format: TextureFormat::Bgra8Unorm,
                present_mode: settings.present_mode,
                last_render_time: Instant::now(),
                frame: 0,
                global_data,
                final_image,
                depth_buffer,
            },
            factory,
        )
    }

    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        _: Queries<()>,
        res: Res<(Read<RendererSettings>,)>,
    ) {
        let res = res.get();
        let settings = res.0.unwrap();

        // See if rendering needs to be performed
        let now = Instant::now();
        let do_render = if let Some(render_time) = settings.render_time {
            now.duration_since(self.last_render_time) >= render_time
        } else {
            true
        };

        // Send events
        if do_render {
            let dur = now.duration_since(self.last_render_time);
            self.last_render_time = now;
            commands.events.submit(PreRender(dur));
            commands.events.submit(Render(dur));
            commands.events.submit(PostRender(dur));
        }
    }

    fn render(
        &mut self,
        _: Render,
        _: Commands,
        queries: Queries<RenderQuery>,
        res: Res<RenderResources>,
    ) {
        let res = res.get();
        let _settings = res.0.unwrap();
        let windows = res.1.unwrap();
        let factory = res.2.unwrap();

        // Check if the window is minimized. If it is, we should skip rendering
        let window = windows
            .get(self.surface_window)
            .expect("surface window is destroyed");
        if window.physical_height() == 0 || window.physical_width() == 0 {
            return;
        }

        // Move to the next frame
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;

        // Process factory resources
        factory.process(self.frame);

        // Acquire an image from our surface
        let surface_image = self.surface.acquire_image().unwrap();

        // Perform our main pass
        let mut commands = self.ctx.main().command_buffer();

        // Prepare global object data
        self.global_data
            .prepare_object_data(self.frame, &factory, &queries);

        // Prepare rendering data
        let mut cameras = factory.0.cameras.lock().unwrap();
        let active_cameras = factory.0.active_cameras.lock().unwrap();
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get_mut(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            // Prepare IDs and draw calls
            camera
                .render_data
                .prepare_input_ids(self.frame, camera.descriptor.layers, &queries);
            camera.render_data.prepare_draw_calls(self.frame, &factory);

            // Update sets
            camera
                .render_data
                .update_draw_gen_set(&self.global_data, self.frame);
            camera
                .render_data
                .update_global_set(&self.global_data, self.frame);
        }

        // Generate draw calls
        // NOTE: We are generating all draw calls first and then performing rendering instead of
        // doing both at the same time because your GPU will cry if you schedule a ton of both
        // compute and graphics work at the same time
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };
            camera
                .render_data
                .generate_draw_calls(self.frame, &self.global_data, &mut commands);
        }

        // Grab resources for rendering
        let texture_sets = factory.0.texture_sets.lock().unwrap();
        let material_buffers = factory.0.material_buffers.lock().unwrap();
        let mesh_buffers = factory.0.mesh_buffers.lock().unwrap();
        let materials = factory.0.materials.lock().unwrap();
        let material_instances = factory.0.material_instances.lock().unwrap();
        let meshes = factory.0.meshes.lock().unwrap();

        // Render from every camera
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            let load_op = match camera.descriptor.clear_color {
                Some(clear) => LoadOp::Clear(ClearColor::RgbaF32(clear.x, clear.y, clear.z, 0.0)),
                None => LoadOp::Load,
            };

            // Perform the depth prepass
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![],
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &self.depth_buffer,
                        array_element: self.frame,
                        mip_level: 0,
                        load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                        store_op: StoreOp::Store,
                    }),
                },
                |pass| {
                    let draw_count = camera.render_data.keys[self.frame].len();
                    camera.render_data.render(
                        self.frame,
                        RenderArgs {
                            pass,
                            texture_sets: &texture_sets,
                            material_buffers: &material_buffers,
                            mesh_buffers: &mesh_buffers,
                            materials: &materials,
                            material_instances: &material_instances,
                            meshes: &meshes,
                            pipeline_ty: PipelineType::DepthOnly,
                            draw_offset: 0,
                            draw_count,
                        },
                    );
                },
            );

            // Perform opaque rendering
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![ColorAttachment {
                        source: ColorAttachmentSource::Texture {
                            texture: &self.final_image,
                            array_element: self.frame,
                            mip_level: 0,
                        },
                        load_op,
                        store_op: StoreOp::Store,
                    }],
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &self.depth_buffer,
                        array_element: self.frame,
                        mip_level: 0,
                        load_op: LoadOp::Load,
                        store_op: StoreOp::DontCare,
                    }),
                },
                |pass| {
                    let draw_count = camera.render_data.keys[self.frame].len();
                    camera.render_data.render(
                        self.frame,
                        RenderArgs {
                            pass,
                            texture_sets: &texture_sets,
                            material_buffers: &material_buffers,
                            mesh_buffers: &mesh_buffers,
                            materials: &materials,
                            material_instances: &material_instances,
                            meshes: &meshes,
                            pipeline_ty: PipelineType::Opaque,
                            draw_offset: 0,
                            draw_count,
                        },
                    );
                },
            );
        }

        // Blit the final image onto the surface image
        let (surface_width, surface_height) = self.surface.dimensions();
        commands.blit_texture(
            &self.final_image,
            BlitDestination::SurfaceImage(&surface_image),
            Blit {
                src_min: (0, 0, 0),
                src_max: self.final_image.dims(),
                src_mip: 0,
                src_array_element: self.frame,
                dst_min: (0, 0, 0),
                dst_max: (surface_width, surface_height, 1),
                dst_mip: 0,
                dst_array_element: 0,
            },
            Filter::Linear,
        );

        // Submit for rendering
        self.ctx.main().submit(Some("main_pass"), commands);

        // Submit the surface image for presentation
        if let SurfacePresentSuccess::Invalidated = self
            .ctx
            .present()
            .present(&self.surface, surface_image)
            .unwrap()
        {
            self.surface
                .update_config(SurfaceConfiguration {
                    width: window.physical_width(),
                    height: window.physical_height(),
                    present_mode: self.present_mode,
                    format: self.surface_format,
                })
                .unwrap();
        }
    }
}

impl From<Renderer> for System {
    fn from(renderer: Renderer) -> System {
        SystemBuilder::new(renderer)
            .with_handler(Renderer::tick)
            .with_handler(Renderer::render)
            .build()
    }
}

impl Default for RendererSettings {
    fn default() -> Self {
        RendererSettings {
            render_scene: true,
            render_time: Some(Duration::from_secs_f32(1.0 / 60.0)),
            canvas_size: None,
            anisotropy_level: None,
            present_mode: PresentMode::Fifo,
        }
    }
}
