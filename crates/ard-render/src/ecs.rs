use ard_log::info;
use ard_math::{Mat4, Vec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::{
    active::ActiveCamera, froxels::FroxelGenPipeline, ubo::CameraUbo, Camera, CameraClearColor,
};
use ard_render_image_effects::{
    ao::{AmbientOcclusion, AoSettings},
    bloom::Bloom,
    fxaa::Fxaa,
    smaa::Smaa,
    sun_shafts2::SunShafts,
    tonemapping::Tonemapping,
};
use ard_render_lighting::{lights::LightClusters, proc_skybox::ProceduralSkyBox};
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{Model, RenderFlags};
use ard_render_renderers::{
    gui::{GuiDrawPrepare, GuiRenderer},
    highz::HzbRenderer,
    raytrace::RaytracedRenderer,
    scene::{SceneRenderArgs, SceneRenderer},
    shadow::{ShadowRenderArgs, SunShadowsRenderer},
};
use ard_render_si::{bindings::Layouts, consts::*};
use ard_render_textures::factory::TextureFactory;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{canvas::Canvas, factory::Factory, frame::FrameData, RenderPlugin, FRAMES_IN_FLIGHT};

pub(crate) struct RenderEcs {
    layouts: Layouts,
    canvas: Canvas,
    camera: CameraUbo,
    scene_renderer: SceneRenderer,
    sun_shadows_renderer: SunShadowsRenderer,
    hzb_render: HzbRenderer,
    rt_render: RaytracedRenderer,
    gui_renderer: GuiRenderer,
    lighting: LightClusters,
    froxels: FroxelGenPipeline,
    _fxaa: Fxaa,
    smaa: Smaa,
    bloom: Bloom<FRAMES_IN_FLIGHT>,
    sun_shafts: SunShafts,
    tonemapping: Tonemapping,
    ao: AmbientOcclusion,
    proc_skybox: ProceduralSkyBox,
    factory: Factory,
    ctx: Context,
}

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

impl RenderEcs {
    pub fn new<W: HasRawWindowHandle + HasRawDisplayHandle>(
        plugin: RenderPlugin,
        window: &W,
        window_size: (u32, u32),
    ) -> (Self, Factory) {
        // Initialize the backend based on what renderer we want
        let backend = {
            ard_pal::backend::VulkanBackend::new(ard_pal::backend::VulkanBackendCreateInfo {
                app_name: String::from("ard"),
                engine_name: String::from("ard"),
                window,
                debug: plugin.debug,
            })
            .unwrap()
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
                    present_mode: plugin.settings.present_mode,
                    format: Format::Bgra8Unorm,
                },
                window,
                debug_name: Some(String::from("primary_surface")),
            },
        )
        .unwrap();

        let layouts = Layouts::new(&ctx);
        let factory = Factory::new(ctx.clone(), &layouts);
        let hzb_render = HzbRenderer::new(&ctx, &layouts);
        let fxaa = Fxaa::new(&ctx, &layouts);
        let ao = AmbientOcclusion::new(&ctx, &layouts);
        let lighting = LightClusters::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        let canvas = Canvas::new(
            &ctx,
            surface,
            window_size,
            plugin.settings.present_mode,
            &hzb_render,
            &ao,
        );

        let mut scene_renderer = SceneRenderer::new(&ctx, &layouts, FRAMES_IN_FLIGHT);
        let sun_shadows_renderer =
            SunShadowsRenderer::new(&ctx, &layouts, FRAMES_IN_FLIGHT, MAX_SHADOW_CASCADES);
        let gui_renderer = GuiRenderer::new(&ctx, &layouts, FRAMES_IN_FLIGHT);
        let rt_render = RaytracedRenderer::new(&ctx, FRAMES_IN_FLIGHT);

        let proc_skybox = ProceduralSkyBox::new(&ctx, &layouts, FRAMES_IN_FLIGHT);
        let bloom = Bloom::new(&ctx, &layouts, window_size, 6);
        let sun_shafts = SunShafts::new(&ctx, &layouts, FRAMES_IN_FLIGHT, window_size);
        let smaa = Smaa::new(&ctx, &layouts, FRAMES_IN_FLIGHT, window_size);
        let mut tonemapping = Tonemapping::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        for frame in 0..FRAMES_IN_FLIGHT {
            let frame = Frame::from(frame);

            tonemapping.bind_bloom(frame, bloom.image());
            tonemapping.bind_sun_shafts(frame, sun_shafts.image());

            scene_renderer
                .color_pass_sets_mut()
                .update_sun_shadow_bindings(frame, &sun_shadows_renderer);

            scene_renderer
                .transparent_pass_sets_mut()
                .update_sun_shadow_bindings(frame, &sun_shadows_renderer);

            scene_renderer
                .color_pass_sets_mut()
                .update_sky_box_bindings(frame, &proc_skybox);

            scene_renderer
                .transparent_pass_sets_mut()
                .update_sky_box_bindings(frame, &proc_skybox);

            scene_renderer
                .color_pass_sets_mut()
                .update_ao_image_binding(frame, canvas.ao().texture());

            scene_renderer
                .transparent_pass_sets_mut()
                .update_ao_image_binding(frame, canvas.ao().texture());

            scene_renderer
                .color_pass_sets_mut()
                .update_light_clusters_binding(frame, &lighting);

            scene_renderer
                .transparent_pass_sets_mut()
                .update_light_clusters_binding(frame, &lighting);
        }

        (
            Self {
                froxels: FroxelGenPipeline::new(&ctx, &layouts),
                canvas,
                camera: CameraUbo::new(&ctx, FRAMES_IN_FLIGHT, true, &layouts),
                scene_renderer,
                sun_shadows_renderer,
                rt_render,
                gui_renderer,
                lighting,
                hzb_render,
                _fxaa: fxaa,
                smaa,
                sun_shafts,
                ao,
                tonemapping,
                bloom,
                proc_skybox,
                layouts,
                factory: factory.clone(),
                ctx,
            },
            factory,
        )
    }

    #[inline(always)]
    pub fn ctx(&self) -> &Context {
        &self.ctx
    }

    pub fn render(&mut self, mut frame: FrameData) -> FrameData {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_function!();

        // Upload factory resources
        self.factory.process(frame.frame);

        // Update the canvas size and acquire a new swap chain image
        if self.canvas.resize(
            &self.ctx,
            &self.hzb_render,
            &self.ao,
            frame.canvas_size,
            frame.msaa_settings.samples,
        ) {
            self.bloom.resize(&self.ctx, frame.canvas_size, 6);
            self.sun_shafts.resize(&self.ctx, frame.canvas_size);
            self.smaa.resize(&self.ctx, frame.canvas_size);

            for frame in 0..FRAMES_IN_FLIGHT {
                let frame = Frame::from(frame);
                self.tonemapping.bind_bloom(frame, self.bloom.image());
                self.tonemapping
                    .bind_sun_shafts(frame, self.sun_shafts.image());
                self.scene_renderer
                    .color_pass_sets_mut()
                    .update_ao_image_binding(frame, self.canvas.ao().texture());
                self.scene_renderer
                    .transparent_pass_sets_mut()
                    .update_ao_image_binding(frame, self.canvas.ao().texture());
            }
        }

        // Update shadow cascades if needed
        let new_shadow_cascades = self.sun_shadows_renderer.update_cascade_settings(
            &self.ctx,
            &self.layouts,
            FRAMES_IN_FLIGHT,
            frame.lights.global().shadow_cascades(),
        );

        if new_shadow_cascades {
            for i in 0..FRAMES_IN_FLIGHT {
                let frame = Frame::from(i);
                self.scene_renderer
                    .color_pass_sets_mut()
                    .update_sun_shadow_bindings(frame, &self.sun_shadows_renderer);
                self.scene_renderer
                    .transparent_pass_sets_mut()
                    .update_sun_shadow_bindings(frame, &self.sun_shadows_renderer);
            }
        }

        // Update lights if needed
        if frame.lights.buffer_expanded() || new_shadow_cascades {
            self.scene_renderer
                .color_pass_sets_mut()
                .update_lights_binding(frame.frame, &frame.lights);
            self.scene_renderer
                .transparent_pass_sets_mut()
                .update_lights_binding(frame.frame, &frame.lights);
        }

        self.sun_shafts.update_binds(
            frame.frame,
            frame.lights.global_buffer(),
            self.sun_shadows_renderer.sun_shadow_info(frame.frame),
            std::array::from_fn(|i| {
                self.sun_shadows_renderer
                    .shadow_cascade(i)
                    .unwrap_or_else(|| self.sun_shadows_renderer.empty_shadow())
            }),
            self.canvas.render_target().depth_resolve(),
        );

        self.smaa
            .update_bindings(frame.frame, self.canvas.render_target().linear_color());

        self.canvas.acquire_image();

        // Update the camera
        let main_camera = match frame.active_cameras.main_camera() {
            Some(camera) => {
                let (width, height) = self.canvas.size();
                self.camera
                    .update(frame.frame, &camera.camera, width, height, camera.model);
                camera
            }
            None => &DEFAULT_ACTIVE_CAMERA,
        };

        let view_location = main_camera.model.position();

        let meshes = self.factory.inner.meshes.lock().unwrap();
        let mesh_factory = self.factory.inner.mesh_factory.lock().unwrap();
        let materials = self.factory.inner.materials.lock().unwrap();
        let texture_factory = self.factory.inner.texture_factory.lock().unwrap();
        let material_factory = self.factory.inner.material_factory.lock().unwrap();
        let mut pending_blas = self.factory.inner.pending_blas.lock().unwrap();

        // Check objects for uploaded BLAS'
        frame.object_data.check_for_blas(&meshes);

        // Upload object data to renderers
        self.scene_renderer.upload(
            frame.frame,
            &frame.object_data,
            &meshes,
            &materials,
            view_location,
        );

        self.sun_shadows_renderer.upload(
            frame.frame,
            &frame.object_data,
            &meshes,
            &materials,
            view_location,
        );

        self.rt_render
            .upload(frame.frame, view_location, &frame.object_data, &meshes);

        // Update sets and bindings
        self.lighting.update_set(frame.frame, &frame.lights);

        self.scene_renderer
            .update_bindings(frame.frame, &frame.object_data, self.canvas.hzb());

        self.sun_shadows_renderer
            .update_bindings::<FRAMES_IN_FLIGHT>(frame.frame, &frame.object_data);

        self.sun_shadows_renderer.update_cascade_views(
            frame.frame,
            &main_camera.camera,
            main_camera.model,
            self.canvas.size(),
            frame.lights.global().sun_direction(),
            frame.lights.global().shadow_cascades(),
        );

        self.gui_renderer.prepare(GuiDrawPrepare {
            frame: frame.frame,
            canvas_size: self.canvas.size(),
            gui_output: &mut frame.gui_output,
        });

        self.bloom
            .bind_images(frame.frame, self.canvas.render_target().final_color());
        self.tonemapping.bind_images(
            frame.frame,
            self.canvas.render_target().final_color(),
            self.canvas.render_target().final_depth(),
        );

        // Phase 1:
        //      Main: Render the HZB and skybox for diffuse irradiance.
        //      Comp: Bin lights, generate shadow draw calls.
        let mut main_cb = self.ctx.main().command_buffer();
        // let mut compute_cb = self.ctx.main().command_buffer();

        // Build BLAS'
        pending_blas.to_build().iter().for_each(|blas| {
            let mesh = match meshes.get(blas.mesh_id) {
                Some(mesh) => mesh,
                None => return,
            };

            main_cb.build_bottom_level_acceleration_structure(&mesh.blas, &blas.scratch, 0);
        });

        // Compact BLAS'
        pending_blas
            .to_compact(frame.frame)
            .iter()
            .for_each(|blas| {
                let src = match meshes.get(blas.mesh_id) {
                    Some(src) => &src.blas,
                    None => return,
                };

                let dst = match blas.dst.as_ref() {
                    Some(dst) => dst,
                    None => return,
                };

                main_cb.compact_acceleration_structure(src, dst);
            });

        // Build TLAS
        self.rt_render.build(&mut main_cb, frame.frame);

        // Render the high-z depth image
        self.render_hzb(
            &mut main_cb,
            &frame,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        self.proc_skybox
            .gather_diffuse_irradiance(&mut main_cb, frame.lights.global().sun_direction());

        self.proc_skybox
            .prefilter_environment_map(&mut main_cb, frame.lights.global().sun_direction());

        self.gui_renderer.update_textures(&mut main_cb);

        self.ctx().main().submit(Some("Phase 1"), main_cb);

        // Setup BLAS building/compacting for next frame
        pending_blas.build_next_frame_lists(frame.frame);

        // Phase 2:
        //      Main: Render shadows.
        //      Comp: Generate HZB, generate main draw calls.
        let mut main_cb = self.ctx.main().command_buffer();
        let mut compute_cb = self.ctx.main().command_buffer();

        Self::render_shadows(
            &mut main_cb,
            &frame,
            &self.sun_shadows_renderer,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        self.generate_hzb(&mut compute_cb, &frame);
        self.generate_froxels(&mut compute_cb, &frame);
        self.cluster_lights(&mut compute_cb, &frame);

        self.ctx()
            .main()
            .submit_with_async_compute(Some("Phase 2"), main_cb, compute_cb);

        // Phase 3:
        // Depth prepass and AO gen.
        let mut cb = self.ctx.main().command_buffer();

        // Perform the depth prepass
        Self::depth_prepass(
            &mut cb,
            &frame,
            &self.canvas,
            &self.camera,
            &self.scene_renderer,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Generate the AO image
        Self::generate_ao_image(
            &mut cb,
            &frame,
            &self.canvas,
            &self.camera,
            &self.ao,
            &frame.ao_settings,
        );

        // Hand off sun shafts for async compute
        self.sun_shafts
            .transfer_image_ownership(&mut cb, QueueType::Compute);

        self.ctx.main().submit(Some("Phase 3"), cb);

        // Phase 4:
        //      Main: Opaque and transparent passes.
        //      Comp: Generate sun shafts.
        let mut main_cb = self.ctx.main().command_buffer();
        let mut compute_cb = self.ctx.main().command_buffer();

        // Render opaque and alpha masked geometry
        Self::render_opaque(
            &mut main_cb,
            &frame,
            &self.canvas,
            &self.camera,
            &self.scene_renderer,
            &self.proc_skybox,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Render transparent geometry
        Self::render_transparent(
            &mut main_cb,
            &frame,
            &self.canvas,
            &self.camera,
            &self.scene_renderer,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Render sun shafts in async compute. Hand back depth target after rendering.
        self.sun_shafts.render(
            frame.frame,
            &mut compute_cb,
            &self.camera,
            &frame.sun_shafts_settings,
        );
        self.sun_shafts
            .transfer_image_ownership(&mut compute_cb, QueueType::Main);
        compute_cb.transfer_texture_ownership(
            self.canvas.render_target().depth_resolve(),
            0,
            0,
            1,
            QueueType::Main,
            None,
        );

        self.ctx()
            .main()
            .submit_with_async_compute(Some("Phase 4"), main_cb, compute_cb);

        // Phase 5:
        // Image effects/tonemapping and final output.
        let mut cb = self.ctx.main().command_buffer();

        // Apply image effects to the final render target
        self.bloom.render(frame.frame, &mut cb);
        self.tonemapping.render(
            frame.frame,
            &mut cb,
            &self.camera,
            // If SMAA is enabled, we want to draw to the "final color" attachment so it can be
            // read during the SMAA pass.
            if frame.smaa_settings.enabled {
                ColorAttachmentDestination::Texture {
                    texture: self.canvas.render_target().linear_color(),
                    array_element: 0,
                    mip_level: 0,
                }
            }
            // Otherwise, we'll draw directly to the surface
            else {
                ColorAttachmentDestination::SurfaceImage(self.canvas.image())
            },
            &frame.tonemapping_settings,
            frame.dt,
        );

        // Apply anti-aliasing
        if frame.smaa_settings.enabled {
            self.smaa.render(frame.frame, &mut cb, self.canvas.image());
        }

        // Render GUI
        cb.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst: ColorAttachmentDestination::SurfaceImage(self.canvas.image()),
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: None,
                depth_stencil_resolve_attachment: None,
            },
            Some("gui_rendering"),
            |pass| {
                self.gui_renderer
                    .render(frame.frame, self.canvas.size(), pass);
            },
        );

        // Submit for rendering
        frame.job = Some(self.ctx.main().submit(Some("primary"), cb));

        // Present the surface image
        self.canvas.present(&self.ctx, frame.window_size);

        frame
    }

    /// Renders the depth image used to generate the hierarchical-z buffer
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn render_hzb<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory<FRAMES_IN_FLIGHT>,
        texture_factory: &'a TextureFactory,
    ) {
        puffin::profile_function!();

        let (width, height, _) = self.canvas.render_target().color_target().dims();
        let render_area = Vec2::new(width as f32, height as f32);

        commands.render_pass(
            self.canvas.render_target().hzb_pass(),
            Some("render_hzb"),
            |pass| {
                if frame_data.object_data.static_dirty() {
                    info!("Skipping HZB render because static objects are dirty.");
                    return;
                }

                self.scene_renderer.render_hzb(
                    frame_data.frame,
                    SceneRenderArgs {
                        camera: &self.camera,
                        pass,
                        render_area,
                        lock_culling: false,
                        static_dirty: frame_data.object_data.static_dirty(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );

        // Transfer depth so it can be read when generating the HZB
        let depth = self.canvas.render_target().final_depth();
        commands.transfer_texture_ownership(
            depth,
            0,
            0,
            depth.mip_count(),
            QueueType::Compute,
            None,
        );

        // Transfer the HZB image itself.
        self.canvas
            .hzb()
            .transfer_ownership(commands, QueueType::Compute);
    }

    #[inline(never)]
    /// Generates the hierarchical-z buffer for use in draw call generation.
    fn generate_hzb<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        puffin::profile_function!();

        self.hzb_render
            .generate(frame_data.frame, commands, self.canvas.hzb());

        let depth = self.canvas.render_target().final_depth();
        commands.transfer_texture_ownership(depth, 0, 0, depth.mip_count(), QueueType::Main, None);
        self.canvas
            .hzb()
            .transfer_ownership(commands, QueueType::Main);
    }

    #[inline(never)]
    /// Generates camera froxels for light clustering.
    fn generate_froxels<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        puffin::profile_function!();

        if !self.camera.needs_froxel_regen() {
            return;
        }

        info!("Generating camera froxels.");
        self.froxels.regen(frame_data.frame, commands, &self.camera);
    }

    #[inline(never)]
    // Perform light clustering
    fn cluster_lights<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        puffin::profile_function!();

        self.lighting
            .cluster(commands, frame_data.frame, &self.camera);
    }

    #[inline(never)]
    /// Performs the entire depth prepass.
    #[allow(clippy::too_many_arguments)]
    fn depth_prepass<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        scene_render: &'a SceneRenderer,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory<FRAMES_IN_FLIGHT>,
        texture_factory: &'a TextureFactory,
    ) {
        puffin::profile_function!();

        let (width, height, _) = canvas.render_target().color_target().dims();
        let render_area = Vec2::new(width as f32, height as f32);

        commands.render_pass(
            canvas.render_target().depth_prepass(),
            Some("depth_prepass"),
            |pass| {
                scene_render.render_depth_prepass(
                    frame_data.frame,
                    SceneRenderArgs {
                        pass,
                        camera,
                        render_area,
                        lock_culling: frame_data.debug_settings.lock_culling,
                        static_dirty: frame_data.object_data.static_dirty(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );

        canvas.render_target().copy_depth(commands);
    }

    #[inline(never)]
    fn generate_ao_image<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        ao: &'a AmbientOcclusion,
        settings: &AoSettings,
    ) {
        puffin::profile_function!();

        ao.generate(frame_data.frame, commands, canvas.ao(), camera, settings);

        // Hand off depth copy image to compute for sun shafts
        commands.transfer_texture_ownership(
            canvas.render_target().depth_resolve(),
            0,
            0,
            1,
            QueueType::Compute,
            None,
        );
    }

    #[inline(never)]
    /// Performs shadow rendering
    #[allow(clippy::too_many_arguments)]
    fn render_shadows<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        shadow_renderer: &'a SunShadowsRenderer,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory<FRAMES_IN_FLIGHT>,
        texture_factory: &'a TextureFactory,
    ) {
        puffin::profile_function!();

        for cascade in 0..shadow_renderer.cascade_count() {
            shadow_renderer.render(
                frame_data.frame,
                ShadowRenderArgs {
                    commands,
                    mesh_factory,
                    material_factory,
                    texture_factory,
                    meshes,
                    materials,
                    cascade,
                },
            );
        }
    }

    #[inline(never)]
    /// Renders opaque and alpha cutout geometry
    #[allow(clippy::too_many_arguments)]
    fn render_opaque<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        scene_render: &'a SceneRenderer,
        proc_skybox: &'a ProceduralSkyBox,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory<FRAMES_IN_FLIGHT>,
        texture_factory: &'a TextureFactory,
    ) {
        puffin::profile_function!();

        let (width, height, _) = canvas.render_target().color_target().dims();
        let render_area = Vec2::new(width as f32, height as f32);

        commands.render_pass(
            canvas
                .render_target()
                .opaque_pass(CameraClearColor::Color(Vec4::ZERO)),
            Some("opaque_pass"),
            |pass| {
                scene_render.render_opaque(
                    frame_data.frame,
                    SceneRenderArgs {
                        pass,
                        camera,
                        render_area,
                        lock_culling: false,
                        static_dirty: frame_data.object_data.static_dirty(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );

                proc_skybox.render(
                    pass,
                    camera.get_set(frame_data.frame),
                    frame_data.lights.global().sun_direction(),
                );
            },
        );
    }

    #[inline(never)]
    /// Renders transparent geometry
    #[allow(clippy::too_many_arguments)]
    fn render_transparent<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        scene_render: &'a SceneRenderer,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory<FRAMES_IN_FLIGHT>,
        texture_factory: &'a TextureFactory,
    ) {
        puffin::profile_function!();

        let (width, height, _) = canvas.render_target().color_target().dims();
        let render_area = Vec2::new(width as f32, height as f32);

        commands.render_pass(
            canvas.render_target().transparent_pass(),
            Some("transparent_pass"),
            |pass| {
                scene_render.render_transparent(
                    frame_data.frame,
                    SceneRenderArgs {
                        pass,
                        camera,
                        render_area,
                        lock_culling: frame_data.debug_settings.lock_culling,
                        static_dirty: frame_data.object_data.static_dirty(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );
    }
}
