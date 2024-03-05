use ard_log::info;
use ard_math::{Mat4, Vec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::{
    active::ActiveCamera, froxels::FroxelGenPipeline, ubo::CameraUbo, Camera, CameraClearColor,
};
use ard_render_image_effects::{
    ao::AmbientOcclusion, bloom::Bloom, fxaa::Fxaa, sun_shafts2::SunShafts,
    tonemapping::Tonemapping,
};
use ard_render_lighting::{lights::Lighting, proc_skybox::ProceduralSkyBox};
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{Model, RenderFlags};
use ard_render_renderers::{
    draw_gen::DrawGenPipeline,
    gui::{GuiDrawPrepare, GuiRenderer},
    highz::HzbRenderer,
    scene::{SceneRenderArgs, SceneRenderer},
    shadow::{ShadowRenderArgs, SunShadowsRenderer},
};
use ard_render_si::bindings::Layouts;
use ard_render_textures::factory::TextureFactory;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{canvas::Canvas, factory::Factory, frame::FrameData, RenderPlugin, FRAMES_IN_FLIGHT};

pub(crate) struct RenderEcs {
    _layouts: Layouts,
    canvas: Canvas,
    camera: CameraUbo,
    scene_renderer: SceneRenderer,
    sun_shadows_renderer: SunShadowsRenderer,
    gui_renderer: GuiRenderer,
    lighting: Lighting,
    froxels: FroxelGenPipeline,
    draw_gen: DrawGenPipeline,
    fxaa: Fxaa,
    hzb_render: HzbRenderer,
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
        let draw_gen = DrawGenPipeline::new(&ctx, &layouts);
        let hzb_render = HzbRenderer::new(&ctx, &layouts);
        let fxaa = Fxaa::new(&ctx, &layouts);
        let ao = AmbientOcclusion::new(&ctx, &layouts);
        let lighting = Lighting::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        let canvas = Canvas::new(
            &ctx,
            surface,
            window_size,
            plugin.settings.anti_aliasing,
            plugin.settings.present_mode,
            &hzb_render,
            &ao,
        );

        let mut scene_renderer = SceneRenderer::new(&ctx, &layouts, &draw_gen, FRAMES_IN_FLIGHT);
        let sun_shadows_renderer =
            SunShadowsRenderer::new(&ctx, &layouts, &draw_gen, &lighting, FRAMES_IN_FLIGHT, 4);
        let gui_renderer = GuiRenderer::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        let proc_skybox = ProceduralSkyBox::new(&ctx, &layouts, FRAMES_IN_FLIGHT);
        let bloom = Bloom::new(&ctx, &layouts, window_size, 6);
        let sun_shafts = SunShafts::new(&ctx, &layouts, FRAMES_IN_FLIGHT, window_size);
        let mut tonemapping = Tonemapping::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        for frame in 0..FRAMES_IN_FLIGHT {
            let frame = Frame::from(frame);

            tonemapping.bind_bloom(frame, bloom.image());
            tonemapping.bind_sun_shafts(frame, sun_shafts.image());

            scene_renderer.global_sets_mut().update_shadow_bindings(
                frame,
                sun_shadows_renderer.sun_shadow_info(frame),
                std::array::from_fn(|i| {
                    sun_shadows_renderer
                        .shadow_cascade(i)
                        .unwrap_or_else(|| sun_shadows_renderer.empty_shadow())
                }),
            );

            scene_renderer
                .global_sets_mut()
                .update_di_map_binding(frame, proc_skybox.di_map());

            scene_renderer
                .global_sets_mut()
                .update_ao_image_binding(frame, canvas.ao().texture());

            scene_renderer
                .global_sets_mut()
                .update_light_clusters_binding(frame, lighting.clusters());
        }

        (
            Self {
                froxels: FroxelGenPipeline::new(&ctx, &layouts),
                canvas,
                camera: CameraUbo::new(&ctx, FRAMES_IN_FLIGHT, true, &layouts),
                scene_renderer,
                sun_shadows_renderer,
                gui_renderer,
                lighting,
                draw_gen,
                hzb_render,
                fxaa,
                sun_shafts,
                ao,
                tonemapping,
                bloom,
                proc_skybox,
                _layouts: layouts,
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

        // Update the canvas size and acquire a new swap chain image
        if self
            .canvas
            .resize(&self.ctx, &self.hzb_render, &self.ao, frame.canvas_size)
        {
            self.bloom.resize(&self.ctx, frame.canvas_size, 6);
            self.sun_shafts.resize(&self.ctx, frame.canvas_size);

            for frame in 0..FRAMES_IN_FLIGHT {
                let frame = Frame::from(frame);
                self.tonemapping.bind_bloom(frame, self.bloom.image());
                self.tonemapping
                    .bind_sun_shafts(frame, self.sun_shafts.image());
                self.scene_renderer
                    .global_sets_mut()
                    .update_ao_image_binding(frame, self.canvas.ao().texture());
            }
        }

        // Update lights if needed
        if frame.lights.buffer_expanded() {
            self.scene_renderer
                .global_sets_mut()
                .update_lighting_binding(
                    frame.frame,
                    frame.lights.global_buffer(),
                    frame.lights.buffer(),
                );

            self.sun_shadows_renderer
                .update_cascade_lights(frame.frame, &frame.lights);
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
            self.canvas.render_target().depth(),
        );

        self.canvas.acquire_image();

        // Upload factory resources
        self.factory.process(frame.frame);

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

        // Update sets and bindings
        self.lighting.update_set(frame.frame, &frame.lights);

        self.scene_renderer.update_bindings(
            frame.frame,
            &frame.object_data,
            self.canvas.hzb(),
            &mesh_factory,
        );

        self.sun_shadows_renderer
            .update_bindings::<FRAMES_IN_FLIGHT>(frame.frame, &frame.object_data, &mesh_factory);

        self.sun_shadows_renderer.update_cascade_views(
            frame.frame,
            &main_camera.camera,
            main_camera.model,
            self.canvas.size(),
            frame.lights.global().sun_direction(),
        );

        self.gui_renderer.prepare(GuiDrawPrepare {
            frame: frame.frame,
            canvas_size: self.canvas.size(),
            gui_output: &mut frame.gui_output,
        });

        self.bloom
            .bind_images(frame.frame, self.canvas.render_target().color());
        self.tonemapping.bind_images(
            frame.frame,
            self.canvas.render_target().color(),
            self.canvas.render_target().depth(),
        );

        // Phase 1:
        //      Main: Render the HZB and skybox for diffuse irradiance.
        //      Comp: Bin lights, generate shadow draw calls.
        let mut main_cb = self.ctx.main().command_buffer();
        // let mut compute_cb = self.ctx.main().command_buffer();

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

        Self::generate_shadow_draw_calls(
            &mut main_cb,
            &frame,
            &self.sun_shadows_renderer,
            &self.draw_gen,
        );

        self.proc_skybox
            .gather_diffuse_irradiance(&mut main_cb, frame.lights.global().sun_direction());

        self.gui_renderer.update_textures(&mut main_cb);

        self.ctx().main().submit(Some("Phase 1"), main_cb);

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
        self.generate_draw_calls(&mut compute_cb, &frame);
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
        Self::generate_ao_image(&mut cb, &frame, &self.canvas, &self.camera, &self.ao);

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
        self.sun_shafts
            .render(frame.frame, &mut compute_cb, &self.camera);
        self.sun_shafts
            .transfer_image_ownership(&mut compute_cb, QueueType::Main);
        compute_cb.transfer_texture_ownership(
            self.canvas.render_target().depth(),
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

        self.bloom.render(frame.frame, &mut cb);
        self.tonemapping
            .render(frame.frame, &mut cb, &self.camera, self.canvas.image());

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
                        static_dirty: frame_data.object_data.static_dirty(),
                        global: self.scene_renderer.global_sets(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );

        let depth = self.canvas.render_target().depth();
        commands.transfer_texture_ownership(
            depth,
            0,
            0,
            depth.mip_count(),
            QueueType::Compute,
            None,
        );
    }

    #[inline(never)]
    /// Generates the hierarchical-z buffer for use in draw call generation.
    fn generate_hzb<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        puffin::profile_function!();

        self.hzb_render
            .generate(frame_data.frame, commands, self.canvas.hzb());

        let depth = self.canvas.render_target().depth();
        commands.transfer_texture_ownership(depth, 0, 0, depth.mip_count(), QueueType::Main, None);
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
    /// Generates draw calls.
    fn generate_draw_calls<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        puffin::profile_function!();

        let (width, height) = self.canvas.size();
        self.draw_gen.generate(
            frame_data.frame,
            commands,
            self.scene_renderer.draw_gen_sets(),
            &self.camera,
            Vec2::new(width as f32, height as f32),
        );

        self.draw_gen.compact(
            frame_data.frame,
            commands,
            self.scene_renderer.draw_gen_sets(),
        );

        self.scene_renderer
            .transfer_ownership(frame_data.frame, commands, QueueType::Main);
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

        commands.render_pass(
            canvas.render_target().depth_prepass(),
            Some("depth_prepass"),
            |pass| {
                scene_render.render_depth_prepass(
                    frame_data.frame,
                    SceneRenderArgs {
                        pass,
                        camera,
                        static_dirty: frame_data.object_data.static_dirty(),
                        global: scene_render.global_sets(),
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

    #[inline(never)]
    fn generate_ao_image<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        ao: &'a AmbientOcclusion,
    ) {
        puffin::profile_function!();

        ao.generate(frame_data.frame, commands, canvas.ao(), camera);

        // Hand off depth resolve image to compute for sun shafts
        commands.transfer_texture_ownership(
            canvas.render_target().depth(),
            0,
            0,
            1,
            QueueType::Compute,
            None,
        );
    }

    #[inline(never)]
    fn generate_shadow_draw_calls<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        shadow_renderer: &'a SunShadowsRenderer,
        draw_gen: &'a DrawGenPipeline,
    ) {
        puffin::profile_function!();

        for cascade in 0..shadow_renderer.cascade_count() {
            shadow_renderer.generate_draw_calls(frame_data.frame, commands, draw_gen, cascade);
        }

        for cascade in 0..shadow_renderer.cascade_count() {
            shadow_renderer.compact_draw_calls(frame_data.frame, commands, draw_gen, cascade);
        }

        // for cascade in 0..shadow_renderer.cascade_count() {
        //     shadow_renderer.transfer_ownership(
        //         frame_data.frame,
        //         commands,
        //         cascade,
        //         QueueType::Main,
        //     );
        // }
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

        // for cascade in 0..shadow_renderer.cascade_count() {
        //     shadow_renderer.transfer_ownership(
        //         frame_data.frame,
        //         commands,
        //         cascade,
        //         QueueType::Compute,
        //     );
        // }
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
                        static_dirty: frame_data.object_data.static_dirty(),
                        global: scene_render.global_sets(),
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
                    scene_render.global_sets().get_set(frame_data.frame),
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

        commands.render_pass(
            canvas.render_target().transparent_pass(),
            Some("transparent_pass"),
            |pass| {
                scene_render.render_transparent(
                    frame_data.frame,
                    SceneRenderArgs {
                        pass,
                        camera,
                        static_dirty: frame_data.object_data.static_dirty(),
                        global: scene_render.global_sets(),
                        mesh_factory,
                        material_factory,
                        texture_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );

        scene_render.transfer_ownership(frame_data.frame, commands, QueueType::Compute);
    }
}
