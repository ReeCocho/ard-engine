use ard_ecs::prelude::*;
use ard_log::info;
use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{ecs::RenderPreprocessing, resource::ResourceAllocator};
use ard_render_camera::{
    froxels::FroxelGenPipeline, target::RenderTarget, ubo::CameraUbo, CameraClearColor,
};
use ard_render_image_effects::{
    ao::AmbientOcclusion,
    bloom::Bloom,
    effects::{ImageEffectTextures, ImageEffectsBindImages, ImageEffectsRender},
    tonemapping::Tonemapping,
};
use ard_render_lighting::{lights::Lighting, proc_skybox::ProceduralSkyBox};
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_renderers::{
    draw_gen::DrawGenPipeline,
    highz::HzbRenderer,
    scene::{SceneRenderArgs, SceneRenderer},
    shadow::{ShadowRenderArgs, SunShadowsRenderer},
};
use ard_render_si::bindings::Layouts;
use ard_render_textures::factory::TextureFactory;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{
    canvas::Canvas,
    factory::Factory,
    frame::{FrameData, FrameDataRes},
    RenderPlugin, FRAMES_IN_FLIGHT,
};

use self::{
    camera::CameraSetupSystem, factory::FactoryProcessingSystem, scene_render::SceneRendererSetup,
};

mod camera;
mod factory;
mod scene_render;

pub(crate) struct RenderEcs {
    world: World,
    resources: Resources,
    dispatcher: Dispatcher,
    _layouts: Layouts,
    froxels: FroxelGenPipeline,
    draw_gen: DrawGenPipeline,
    hzb_render: HzbRenderer,
    effect_textures: ImageEffectTextures,
    bloom: Bloom<FRAMES_IN_FLIGHT>,
    tonemapping: Tonemapping,
    ao: AmbientOcclusion,
    proc_skybox: ProceduralSkyBox,
    factory: Factory,
    ctx: Context,
}

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

        // Create layouts resource
        let layouts = Layouts::new(&ctx);

        // Create the resource factory
        let factory = Factory::new(ctx.clone(), &layouts);

        let hzb_render = HzbRenderer::new(&ctx, &layouts);
        let ao = AmbientOcclusion::new(&ctx, &layouts);
        let draw_gen = DrawGenPipeline::new(&ctx, &layouts);
        let proc_skybox = ProceduralSkyBox::new(&ctx, &layouts);

        // Define resources used in the render ECS
        let mut resources = Resources::default();
        resources.add(FrameDataRes::default());
        resources.add(Canvas::new(
            &ctx,
            surface,
            window_size,
            plugin.settings.anti_aliasing,
            plugin.settings.present_mode,
            &hzb_render,
            &ao,
        ));
        resources.add(CameraUbo::new(&ctx, FRAMES_IN_FLIGHT, true, &layouts));
        resources.add(layouts.clone());
        resources.add(factory.clone());
        resources.add(SceneRenderer::new(
            &ctx,
            &layouts,
            &draw_gen,
            FRAMES_IN_FLIGHT,
        ));
        resources.add(SunShadowsRenderer::new(
            &ctx,
            &layouts,
            &draw_gen,
            FRAMES_IN_FLIGHT,
            4,
        ));
        resources.add(Lighting::new(&ctx, &layouts, FRAMES_IN_FLIGHT));

        (
            Self {
                froxels: FroxelGenPipeline::new(&ctx, &layouts),
                draw_gen,
                hzb_render,
                ao,
                effect_textures: ImageEffectTextures::new(
                    &ctx,
                    RenderTarget::COLOR_FORMAT,
                    Format::Bgra8Unorm,
                    window_size,
                ),
                tonemapping: Tonemapping::new(&ctx, &layouts, FRAMES_IN_FLIGHT),
                bloom: Bloom::new(&ctx, &layouts, window_size, 6),
                proc_skybox,
                world: World::default(),
                resources,
                dispatcher: DispatcherBuilder::new()
                    // TODO: Configurable thread count
                    .thread_count(4)
                    .add_system(FactoryProcessingSystem)
                    .add_system(CameraSetupSystem)
                    .add_system(SceneRendererSetup)
                    .build(),
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
        // Update the canvas size and acquire a new swap chain image
        let mut canvas = self.resources.get_mut::<Canvas>().unwrap();
        if canvas.resize(
            &self.ctx,
            &self.hzb_render,
            &self.ao,
            &mut self.effect_textures,
            frame.canvas_size,
        ) {
            self.bloom.resize(&self.ctx, frame.canvas_size, 6);
        }

        self.tonemapping.bind_bloom(frame.frame, self.bloom.image());

        canvas.acquire_image();

        std::mem::drop(canvas);

        // Perform preprocessing
        self.world.process_entities();
        self.dispatcher
            .event_sender()
            .submit(RenderPreprocessing(frame.frame));
        self.resources
            .get_mut::<FrameDataRes>()
            .unwrap()
            .insert(frame);
        self.dispatcher.run(&mut self.world, &self.resources);
        self.world.process_entities();

        // Run the render graph to perform primary rendering
        let mut cb = self.ctx.main().command_buffer();

        frame = self.resources.get_mut::<FrameDataRes>().unwrap().take();
        let mut canvas = self.resources.get_mut::<Canvas>().unwrap();

        ImageEffectsBindImages::new(&self.effect_textures)
            .add(&mut self.bloom)
            .add(&mut self.tonemapping)
            .bind(
                frame.frame,
                canvas.render_target().color(),
                canvas.render_target().depth(),
                canvas.image(),
            );

        let meshes = self.factory.inner.meshes.lock().unwrap();
        let materials = self.factory.inner.materials.lock().unwrap();
        let mesh_factory = self.factory.inner.mesh_factory.lock().unwrap();
        let texture_factory = self.factory.inner.texture_factory.lock().unwrap();
        let material_factory = self.factory.inner.material_factory.lock().unwrap();
        let scene_render = self.resources.get::<SceneRenderer>().unwrap();
        let shadow_renderer = self.resources.get::<SunShadowsRenderer>().unwrap();
        let camera = self.resources.get::<CameraUbo>().unwrap();
        let lighting = self.resources.get::<Lighting>().unwrap();

        // Render the high-z depth image
        self.render_hzb(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Generate the high-z depth pyramid
        self.generate_hzb(&mut cb, &frame, &canvas);

        // Regenerate froxels
        self.generate_froxels(&mut cb, &frame, &camera);

        // Perform light clustering
        self.cluster_lights(&mut cb, &frame, &lighting, &camera);

        // Generate draw calls
        self.generate_draw_calls(
            &mut cb,
            &frame,
            &canvas,
            &scene_render,
            &shadow_renderer,
            &camera,
        );

        // Perform the depth prepass
        Self::depth_prepass(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Render shadows
        Self::render_shadows(
            &mut cb,
            &frame,
            &shadow_renderer,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Generate the AO image
        Self::generate_ao_image(&mut cb, &frame, &canvas, &camera, &self.ao);

        // Render opaque and alpha masked geometry
        Self::render_opaque(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &self.proc_skybox,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Render transparent geometry
        Self::render_transparent(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Run image effects and blit to surface
        ImageEffectsRender::new(&self.effect_textures)
            .add(&self.bloom)
            .add(&self.tonemapping)
            .render(
                frame.frame,
                &mut cb,
                canvas.render_target().color(),
                canvas.image(),
            );

        // Submit for rendering
        frame.job = Some(self.ctx.main().submit(Some("primary"), cb));

        // Present the surface image
        canvas.present(&self.ctx, frame.window_size);

        frame
    }

    /// Renders the depth image used to generate the hierarchical-z buffer
    #[allow(clippy::too_many_arguments)]
    fn render_hzb<'a>(
        &'a self,
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
        commands.render_pass(canvas.render_target().hzb_pass(), |pass| {
            if frame_data.object_data.static_dirty() {
                info!("Skipping HZB render because static objects are dirty.");
                return;
            }

            scene_render.render_hzb(
                frame_data.frame,
                SceneRenderArgs {
                    camera,
                    pass,
                    static_dirty: frame_data.object_data.static_dirty(),
                    global: scene_render.global_sets(),
                    mesh_factory,
                    material_factory,
                    texture_factory,
                    meshes,
                    materials,
                },
            );
        });
    }

    /// Generates the hierarchical-z buffer for use in draw call generation.
    fn generate_hzb<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
    ) {
        self.hzb_render
            .generate(frame_data.frame, commands, canvas.hzb());
    }

    /// Generates camera froxels for light clustering.
    fn generate_froxels<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        camera: &'a CameraUbo,
    ) {
        if !camera.needs_froxel_regen() {
            return;
        }

        info!("Generating camera froxels.");
        commands.compute_pass(|pass| {
            self.froxels.regen(frame_data.frame, pass, camera);
        });
    }

    // Perform light clustering
    fn cluster_lights<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        lighting: &'a Lighting,
        camera: &'a CameraUbo,
    ) {
        lighting.cluster(commands, frame_data.frame, camera);
    }

    /// Generates draw calls.
    fn generate_draw_calls<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        scene_render: &'a SceneRenderer,
        shadow_renderer: &'a SunShadowsRenderer,
        camera: &'a CameraUbo,
    ) {
        let (width, height) = canvas.size();
        self.draw_gen.generate(
            frame_data.frame,
            commands,
            scene_render.draw_gen_sets(),
            camera,
            Vec2::new(width as f32, height as f32),
        );

        shadow_renderer.generate_draw_calls(frame_data.frame, commands, &self.draw_gen);
    }

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
        commands.render_pass(canvas.render_target().depth_prepass(), |pass| {
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
        });
    }

    fn generate_ao_image<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        ao: &'a AmbientOcclusion,
    ) {
        ao.generate(frame_data.frame, commands, canvas.ao(), camera);
    }

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
        shadow_renderer.render(
            frame_data.frame,
            ShadowRenderArgs {
                commands,
                mesh_factory,
                material_factory,
                texture_factory,
                meshes,
                materials,
            },
        );
    }

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
        commands.render_pass(
            canvas
                .render_target()
                .opaque_pass(CameraClearColor::Color(Vec4::ZERO)),
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
        commands.render_pass(canvas.render_target().transparent_pass(), |pass| {
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
        });
    }
}
