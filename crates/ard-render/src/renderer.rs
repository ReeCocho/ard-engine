use std::time::{Duration, Instant};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_pal::prelude::*;
use ard_window::{
    window::{Window, WindowId},
    windows::Windows,
};
use raw_window_handle::HasRawWindowHandle;

use crate::{factory::Factory, RenderPlugin};

pub const FRAMES_IN_FLIGHT: usize = 2;

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
    final_image: Texture,
    ctx: Context,
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

        // Create the factory
        let factory = Factory::new(ctx.clone());

        (
            Self {
                ctx,
                surface_window: window_id,
                surface,
                surface_format: TextureFormat::Bgra8Unorm,
                present_mode: settings.present_mode,
                last_render_time: Instant::now(),
                frame: 0,
                final_image,
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

    fn render(&mut self, _: Render, _: Commands, _: Queries<()>, res: Res<RenderResources>) {
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
        self.ctx.main().submit(Some("main_pass"), |commands| {
            // Draw the results of our rendering to the surface image
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![ColorAttachment {
                        source: ColorAttachmentSource::SurfaceImage(&surface_image),
                        load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                        store_op: StoreOp::Store,
                    }],
                    depth_stencil_attachment: None,
                },
                |_pass| {},
            );
        });

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
