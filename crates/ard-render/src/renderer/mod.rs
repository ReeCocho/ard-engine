pub mod ao;
pub mod clustering;
pub mod gui;
pub mod occlusion;
pub mod post_process;
pub mod render_data;
pub mod shadows;

use std::{
    ops::DerefMut,
    time::{Duration, Instant},
};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::InputState;
use ard_math::{IVec2, Mat4, Vec2};
use ard_pal::prelude::*;
use ard_window::{window::WindowId, windows::Windows};
use bitflags::bitflags;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use serde::{Deserialize, Serialize};

use crate::{
    camera::{CameraClearColor, CameraUbo},
    factory::Factory,
    lighting::Lighting,
    material::{Material, MaterialInstance, PipelineType},
    mesh::Mesh,
    shader_constants::FRAMES_IN_FLIGHT,
    static_geometry::StaticGeometry,
    RenderPlugin,
};

use self::{
    ao::AmbientOcclusion,
    gui::{Gui, GuiPrepareDraw},
    occlusion::HzbImage,
    post_process::{PostProcessing, PostProcessingSettings},
    render_data::{GlobalRenderData, RenderArgs},
    shadows::{ShadowPrepareArgs, ShadowRenderArgs},
};

#[derive(Resource, Clone)]
pub struct RendererSettings {
    /// Flag to enable drawing the game scene. For games, this should be `true` all the time. This
    /// is useful for things like editors where you only want a GUI.
    pub render_scene: bool,
    /// Time between frame draws. `None` indicates no render limiting.
    pub render_time: Option<Duration>,
    /// Preferred presentation mode.
    pub present_mode: PresentMode,
    /// Super resolution scale factor. A value of `1.0` means no super sampling is performed.
    pub render_scale: f32,
    /// Post processing settings.
    pub post_processing: PostProcessingSettings,
    /// Width and height of the renderer image. `None` indicates the dimensions should match that
    /// of the surface being presented to.
    pub canvas_size: Option<(u32, u32)>,
    /// Anisotropy level to be used for textures. Can be `None` for no filtering.
    pub anisotropy_level: Option<AnisotropyLevel>,
    /// A debugging setting that locks the currently drawn objects in place.
    pub lock_occlusion: bool,
    /// A debugging setting that allows you to override the material of every drawn object.
    pub material_override: Option<Material>,
}

#[derive(SystemState)]
pub struct Renderer {
    surface_window: WindowId,
    surface: Surface,
    surface_format: Format,
    present_mode: PresentMode,
    last_render_time: Instant,
    frame: usize,
    global_data: GlobalRenderData,
    post_processing: PostProcessing,
    ao: AmbientOcclusion,
    depth_buffer: Texture,
    hdr_image: Texture,
    hzb_image: HzbImage,
    use_alternate: [bool; FRAMES_IN_FLIGHT],
    jobs: Vec<Option<Job>>,
    ctx: Context,
}

/// The geometry and material of an object to render.
#[derive(Component, Clone)]
pub struct Renderable {
    pub mesh: Mesh,
    pub material: MaterialInstance,
    pub layers: RenderLayer,
}

/// Model matrix of a renderable object.
#[derive(Component, Default, Serialize, Deserialize, Copy, Clone)]
pub struct Model(pub Mat4);

bitflags! {
    /// Layer mask used for renderable objects.
    #[derive(Debug, Copy, Clone, Hash)]
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

type RendererResources = (
    Read<RendererSettings>,
    Read<InputState>,
    Read<Windows>,
    Write<Gui>,
);

impl Renderer {
    pub fn new<W: HasRawWindowHandle + HasRawDisplayHandle>(
        plugin: RenderPlugin,
        window: &W,
        window_id: WindowId,
        window_size: (u32, u32),
        settings: &RendererSettings,
    ) -> (Self, Factory, StaticGeometry, Lighting, Gui) {
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
                    format: Format::Bgra8Unorm,
                },
                window,
                debug_name: Some(String::from("primary_surface")),
            },
        )
        .unwrap();

        // Create the final texture to copy to the swapchain image
        let (mut width, mut height) = match settings.canvas_size {
            Some(dims) => dims,
            None => window_size,
        };
        width = (width as f32 * settings.render_scale) as u32;
        height = (height as f32 * settings.render_scale) as u32;

        let hdr_image = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba16SFloat,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("hdr_image")),
            },
        )
        .unwrap();

        let depth_buffer = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::D24UnormS8Uint,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("depth_buffer")),
            },
        )
        .unwrap();

        // Create render data
        let global_data = GlobalRenderData::new(&ctx);

        // Create ambient occlusion data
        let ao = AmbientOcclusion::new(&ctx);

        // Create the factory
        let factory = Factory::new(ctx.clone(), settings.anisotropy_level, &global_data, &ao);

        // Create the HZB image
        let hzb_image = factory.0.hzb.new_image(width, height);

        // Create lighting data
        let lighting = Lighting::new(&ctx);

        // Create GUI data
        let gui = Gui::new(ctx.clone(), &factory.0.layouts.textures);

        // Create post processing data
        let post_processing = PostProcessing::new(&ctx, width, height);

        let mut jobs = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            jobs.push(None);
        }

        (
            Self {
                ctx,
                surface_window: window_id,
                surface,
                surface_format: Format::Bgra8Unorm,
                present_mode: settings.present_mode,
                last_render_time: Instant::now(),
                frame: 0,
                global_data,
                post_processing,
                ao,
                hdr_image,
                hzb_image,
                use_alternate: [false; FRAMES_IN_FLIGHT],
                jobs,
                depth_buffer,
            },
            factory,
            StaticGeometry::default(),
            lighting,
            gui,
        )
    }

    #[inline(always)]
    pub fn egui_texture_id() -> egui::TextureId {
        egui::TextureId::User((u32::MAX - 1) as u64)
    }

    fn tick(&mut self, _: Tick, commands: Commands, _: Queries<()>, res: Res<RendererResources>) {
        let settings = res.get::<RendererSettings>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let windows = res.get::<Windows>().unwrap();
        let mut gui = res.get_mut::<Gui>().unwrap();

        // Update input state for egui
        let window = windows.get(self.surface_window).unwrap();
        gui.update_input(&input, window);

        // See if rendering needs to be performed
        let now = Instant::now();
        let do_render = if let Some(render_time) = settings.render_time {
            now.duration_since(self.last_render_time) >= render_time
        } else {
            true
        };

        if do_render {
            // If the next frames job is uncomplete, skip rendering
            if let Some(job) = &self.jobs[(self.frame + 1) % FRAMES_IN_FLIGHT] {
                if job.poll_status() == JobStatus::Running {
                    return;
                }
            }

            // Send events
            let dur = now.duration_since(self.last_render_time);
            self.last_render_time = now;
            commands.events.submit(PreRender(dur));
            commands.events.submit(Render(dur));
            commands.events.submit(PostRender(dur));
        }
    }

    fn render(
        &mut self,
        evt: Render,
        ecs_commands: Commands,
        queries: Queries<Everything>,
        res: Res<Everything>,
    ) {
        let settings = res.get::<RendererSettings>().unwrap();
        let windows = res.get::<Windows>().unwrap();
        let factory = res.get::<Factory>().unwrap();
        let static_geometry = res.get::<StaticGeometry>().unwrap();
        let mut gui = res.get_mut::<Gui>().unwrap();

        // Check if the window is minimized. If it is, we should skip rendering
        let window = windows
            .get(self.surface_window)
            .expect("surface window is destroyed");
        if window.physical_height() == 0 || window.physical_width() == 0 {
            return;
        }

        // Determine the dimensions of the canvas
        let (mut canvas_width, mut canvas_height) = match settings.canvas_size {
            Some(dims) => dims,
            None => self.surface.dimensions(),
        };
        canvas_width = (canvas_width as f32 * settings.render_scale) as u32;
        canvas_height = (canvas_height as f32 * settings.render_scale) as u32;

        // Resize the canvas images if the dimensions don't match
        let (old_width, old_height, _) = self.hdr_image.dims();
        let resized = if old_width != canvas_width || old_height != canvas_height {
            self.hdr_image = Texture::new(
                self.ctx.clone(),
                TextureCreateInfo {
                    format: Format::Rgba16SFloat,
                    ty: TextureType::Type2D,
                    width: canvas_width,
                    height: canvas_height,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some(String::from("hdr_image")),
                },
            )
            .unwrap();
            self.post_processing.resize(canvas_width, canvas_height);
            self.depth_buffer = Texture::new(
                self.ctx.clone(),
                TextureCreateInfo {
                    format: Format::D24UnormS8Uint,
                    ty: TextureType::Type2D,
                    width: canvas_width,
                    height: canvas_height,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some(String::from("depth_buffer")),
                },
            )
            .unwrap();
            self.hzb_image = factory.0.hzb.new_image(canvas_width, canvas_height);
            true
        } else {
            false
        };

        // Move to the next frame
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        self.use_alternate[self.frame] = !self.use_alternate[self.frame];
        let use_alternate = self.use_alternate[self.frame];

        // Cleanup dropped static geometry
        let mut static_geo = static_geometry.0.lock().unwrap();
        static_geo.cleanup();

        // Process factory resources
        factory.process(self.frame);

        // Acquire an image from our surface
        let surface_image = self.surface.acquire_image().unwrap();

        // Perform our main pass
        let mut commands = self.ctx.main().command_buffer();

        // Prepare GUI state
        // NOTE: We have to drop and then re-request a bunch of resources since some GUI views
        // might want access to them.
        let (screen_width, screen_height) = self.surface.dimensions();
        std::mem::drop(settings);
        std::mem::drop(windows);
        std::mem::drop(factory);
        std::mem::drop(static_geo);
        std::mem::drop(static_geometry);
        gui.prepare_draw(GuiPrepareDraw {
            frame: self.frame,
            scene_tex: self.post_processing.final_image(),
            canvas_size: IVec2::new(screen_width as i32, screen_height as i32),
            dt: evt.0.as_secs_f32(),
            commands: &ecs_commands,
            queries: &queries,
            res: &res,
        });
        let settings = res.get::<RendererSettings>().unwrap();
        let windows = res.get::<Windows>().unwrap();
        let window = windows
            .get(self.surface_window)
            .expect("surface window is destroyed");
        let factory = res.get::<Factory>().unwrap();
        let static_geometry = res.get::<StaticGeometry>().unwrap();
        let mut static_geometry = static_geometry.0.lock().unwrap();
        let mut lighting = res.get_mut::<Lighting>().unwrap();

        // Prepare post processing
        self.post_processing.prepare(self.frame, &self.hdr_image);

        // Prepare global object data. Don't prepare if we are locking occlusion
        if !settings.lock_occlusion {
            self.global_data
                .prepare_object_data(self.frame, &factory, &queries, &static_geometry);
            self.global_data.prepare_lights(self.frame, &queries);
        }

        // Prepare rendering data
        let mut cameras = factory.0.cameras.lock().unwrap();
        let cube_maps = factory.0.cube_maps.lock().unwrap();
        let active_cameras = factory.0.active_cameras.lock().unwrap();
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get_mut(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            // If the cavas resized, we need to regen froxels
            if resized {
                camera.mark_froxel_regen();
            }

            // Prepare IDs and draw calls
            if !settings.lock_occlusion {
                camera.render_data.prepare_input_ids(
                    self.frame,
                    camera.descriptor.layers,
                    &queries,
                    &static_geometry,
                );
                camera
                    .render_data
                    .prepare_draw_calls(self.frame, use_alternate, &factory);
            }

            // Update the camera's UBO
            camera.render_data.update_camera_ubo(
                self.frame,
                CameraUbo::new(
                    &camera.descriptor,
                    canvas_width as f32,
                    canvas_height as f32,
                ),
            );

            // Prepare lighting stuff if required
            if let Some(clustering) = &mut camera.render_data.clustering {
                clustering.update_light_clustering_set(self.frame, &self.global_data);
            }

            // Resize AO image if needed
            if camera.descriptor.ao {
                camera.ao.resize_to_fit(canvas_width, canvas_height);
            }

            // Prepare shadows
            camera.shadows.prepare(ShadowPrepareArgs {
                frame: self.frame,
                lighting: lighting.deref_mut(),
                queries: &queries,
                static_geometry: &static_geometry,
                factory: &factory,
                camera: &camera.descriptor,
                use_alternate,
                lock_occlusion: settings.lock_occlusion,
                camera_dims: (canvas_width as f32, canvas_height as f32),
            });

            // Update sets
            camera.render_data.update_draw_gen_set(
                &self.global_data,
                Some(&self.hzb_image),
                self.frame,
                use_alternate,
            );
            camera.render_data.update_global_set(
                &self.global_data,
                &lighting,
                &camera.descriptor.ibl,
                &cube_maps,
                self.frame,
            );
            camera.render_data.update_camera_with_shadows(
                self.frame,
                &self.global_data,
                Some(&camera.shadows),
            );
            camera.render_data.update_camera_ao(
                self.frame,
                if camera.descriptor.ao {
                    camera.ao.texture()
                } else {
                    self.ao.default_texture()
                },
            );
            if let CameraClearColor::SkyBox(sky_box) = &camera.descriptor.clear_color {
                camera
                    .render_data
                    .update_sky_box_set(self.frame, cube_maps.get(sky_box.id).unwrap());
            }
            camera.shadows.update_sets(
                self.frame,
                &self.global_data,
                &lighting,
                &cube_maps,
                use_alternate,
            );
            camera.ao.update_set(
                self.frame,
                &self.ao,
                &camera.render_data.camera_ubo,
                &self.depth_buffer,
            );
        }

        // Update lighting UBO
        lighting.update_ubo(self.frame);

        // Grab resources for rendering
        let texture_sets = factory.0.texture_sets.lock().unwrap();
        let material_buffers = factory.0.material_buffers.lock().unwrap();
        let mesh_buffers = factory.0.mesh_buffers.lock().unwrap();
        let materials = factory.0.materials.lock().unwrap();
        let meshes = factory.0.meshes.lock().unwrap();

        // Render only the static geometry of the previous frame
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![],
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &self.depth_buffer,
                        array_element: 0,
                        mip_level: 0,
                        load_op: LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                        store_op: StoreOp::Store,
                    }),
                },
                |pass| {
                    // Only render if static geometry is the same from last frame
                    if static_geometry.dirty[self.frame] {
                        return;
                    }

                    let draw_count = camera.render_data.last_static_draws;
                    camera.render_data.render(
                        self.frame,
                        !use_alternate,
                        RenderArgs {
                            pass,
                            texture_sets: &texture_sets,
                            material_buffers: &material_buffers,
                            mesh_buffers: &mesh_buffers,
                            materials: &materials,
                            meshes: &meshes,
                            global: &self.global_data,
                            pipeline_ty: PipelineType::DepthOnly,
                            draw_offset: 0,
                            draw_count,
                            draw_sky_box: false,
                            material_override: settings.material_override.clone(),
                        },
                    );
                },
            );
        }

        // Generate the high-z depth pyramid using the depth buffer we just made
        factory.0.hzb.generate(
            self.frame,
            &mut commands,
            &mut self.hzb_image,
            &self.depth_buffer,
        );

        // Regen froxels (if needed) and generate draw calls and light table
        // NOTE: We are generating all draw calls first and then performing rendering instead of
        // doing both at the same time because your GPU will cry if you schedule a ton of both
        // compute and graphics work at the same time
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            // Froxel regen and light clustering
            if let Some(clustering) = &camera.render_data.clustering {
                if camera.needs_froxel_regen(self.frame) {
                    clustering.generate_camera_froxels(
                        self.frame,
                        &self.global_data,
                        &mut commands,
                    );
                }

                clustering.cluster_lights(self.frame, &self.global_data, &mut commands);
            }

            if !settings.lock_occlusion {
                camera.render_data.generate_draw_calls(
                    self.frame,
                    &self.global_data,
                    true,
                    Vec2::new(canvas_width as f32, canvas_height as f32),
                    &mut commands,
                );

                camera
                    .shadows
                    .generate_draw_calls(self.frame, &self.global_data, &mut commands);
            }
        }

        // Render shadows and perform the depth-prepass for every camera
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            // Render shadow maps
            camera.shadows.render(
                self.frame,
                use_alternate,
                ShadowRenderArgs {
                    texture_sets: &texture_sets,
                    material_buffers: &material_buffers,
                    mesh_buffers: &mesh_buffers,
                    materials: &materials,
                    meshes: &meshes,
                    global: &self.global_data,
                },
                &mut commands,
            );

            // Perform the depth prepass
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![],
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &self.depth_buffer,
                        array_element: 0,
                        mip_level: 0,
                        load_op: LoadOp::Load,
                        store_op: StoreOp::Store,
                    }),
                },
                |pass| {
                    let draw_count = camera.render_data.keys[self.frame].len();
                    camera.render_data.render(
                        self.frame,
                        use_alternate,
                        RenderArgs {
                            pass,
                            texture_sets: &texture_sets,
                            material_buffers: &material_buffers,
                            mesh_buffers: &mesh_buffers,
                            materials: &materials,
                            meshes: &meshes,
                            global: &self.global_data,
                            pipeline_ty: PipelineType::DepthOnly,
                            draw_offset: 0,
                            draw_count,
                            draw_sky_box: false,
                            material_override: settings.material_override.clone(),
                        },
                    );
                },
            );
        }

        // Generate ambient occlusion images for each camera
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            if camera.descriptor.ao {
                camera.ao.generate(self.frame, &self.ao, &mut commands);
            }
        }

        // Opaque rendering for every camera
        for camera_id in active_cameras.iter() {
            let camera = match cameras.get(*camera_id) {
                Some(camera) => camera,
                None => continue,
            };

            let (load_op, sky_box) = match &camera.descriptor.clear_color {
                CameraClearColor::None => (LoadOp::DontCare, None),
                CameraClearColor::Color(color) => (
                    LoadOp::Clear(ClearColor::RgbaF32(color.x, color.y, color.z, 0.0)),
                    None,
                ),
                CameraClearColor::SkyBox(sky_box) => (LoadOp::DontCare, Some(sky_box)),
            };

            // Perform opaque rendering
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: vec![ColorAttachment {
                        source: ColorAttachmentSource::Texture {
                            texture: &self.hdr_image,
                            array_element: 0,
                            mip_level: 0,
                        },
                        load_op,
                        store_op: StoreOp::Store,
                    }],
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &self.depth_buffer,
                        array_element: 0,
                        mip_level: 0,
                        load_op: LoadOp::Load,
                        store_op: StoreOp::Store,
                    }),
                },
                |pass| {
                    let draw_count = camera.render_data.keys[self.frame].len();
                    camera.render_data.render(
                        self.frame,
                        use_alternate,
                        RenderArgs {
                            pass,
                            texture_sets: &texture_sets,
                            material_buffers: &material_buffers,
                            mesh_buffers: &mesh_buffers,
                            materials: &materials,
                            meshes: &meshes,
                            global: &self.global_data,
                            pipeline_ty: PipelineType::Opaque,
                            draw_offset: 0,
                            draw_count,
                            draw_sky_box: sky_box.is_some(),
                            material_override: settings.material_override.clone(),
                        },
                    );
                },
            );
        }

        // Update GUI texture data
        gui.update_textures(&mut commands);

        // Post processing
        self.post_processing.draw(
            self.frame,
            Vec2::new(canvas_width as f32, canvas_height as f32),
            &settings.post_processing,
            &mut commands,
        );

        // If required, blit the scene image to the surface
        if settings.render_scene {
            let (src, array_element) = self.post_processing.final_image();
            let (width, height) = self.surface.dimensions();
            commands.blit(
                BlitSource::Texture(src),
                BlitDestination::SurfaceImage(&surface_image),
                Blit {
                    src_min: (0, 0, 0),
                    src_max: src.dims(),
                    src_mip: 0,
                    src_array_element: array_element,
                    dst_min: (0, 0, 0),
                    dst_max: (width, height, 1),
                    dst_mip: 0,
                    dst_array_element: 0,
                },
                Filter::Linear,
            );
        }

        // Render the GUI
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    source: ColorAttachmentSource::SurfaceImage(&surface_image),
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                }],
                depth_stencil_attachment: None,
            },
            |pass| {
                gui.draw(
                    self.frame,
                    Vec2::new(screen_width as f32, screen_height as f32),
                    &texture_sets,
                    pass,
                );
            },
        );

        // Submit for rendering
        self.jobs[self.frame] = Some(self.ctx.main().submit(Some("main_pass"), commands));

        // Mark static geometry as being clean
        static_geometry.dirty[self.frame] = false;

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
            render_scale: 1.0,
            canvas_size: None,
            anisotropy_level: None,
            post_processing: PostProcessingSettings::default(),
            present_mode: PresentMode::Fifo,
            lock_occlusion: false,
            material_override: None,
        }
    }
}
