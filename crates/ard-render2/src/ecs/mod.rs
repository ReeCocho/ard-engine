use ard_ecs::prelude::*;
use ard_log::info;
use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{ecs::RenderPreprocessing, resource::ResourceAllocator};
use ard_render_camera::{froxels::FroxelGenPipeline, ubo::CameraUbo, CameraClearColor};
use ard_render_material::material::MaterialResource;
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_renderers::{
    draw_gen::DrawGenPipeline,
    highz::HzbRenderer,
    scene::{SceneRenderArgs, SceneRenderer},
};
use ard_render_si::bindings::Layouts;
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
    layouts: Layouts,
    froxels: FroxelGenPipeline,
    draw_gen: DrawGenPipeline,
    hzb_render: HzbRenderer,
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
        let draw_gen = DrawGenPipeline::new(&ctx, &layouts);

        // Define resources used in the render ECS
        let mut resources = Resources::default();
        resources.add(FrameDataRes::default());
        resources.add(Canvas::new(
            &ctx,
            surface,
            window_size,
            plugin.settings.present_mode,
            &hzb_render,
        ));
        resources.add(CameraUbo::new(&ctx, FRAMES_IN_FLIGHT, &layouts));
        resources.add(layouts.clone());
        resources.add(factory.clone());
        resources.add(SceneRenderer::new(
            &ctx,
            &layouts,
            &draw_gen,
            FRAMES_IN_FLIGHT,
        ));

        (
            Self {
                froxels: FroxelGenPipeline::new(&ctx, &layouts),
                draw_gen,
                hzb_render,
                world: World::default(),
                resources,
                dispatcher: DispatcherBuilder::new()
                    // TODO: Configurable thread count
                    .thread_count(4)
                    .add_system(FactoryProcessingSystem)
                    .add_system(CameraSetupSystem)
                    .add_system(SceneRendererSetup)
                    .build(),
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
        // Update the canvas size and acquire a new swap chain image
        let mut canvas = self.resources.get_mut::<Canvas>().unwrap();
        canvas.resize(&self.ctx, &self.hzb_render, frame.canvas_size);
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
        self.dispatcher.run(&mut self.world, &mut self.resources);
        self.world.process_entities();

        // Run the render graph to perform primary rendering
        let mut cb = self.ctx.main().command_buffer();

        frame = self.resources.get_mut::<FrameDataRes>().unwrap().take();
        let mut canvas = self.resources.get_mut::<Canvas>().unwrap();
        let meshes = self.factory.inner.meshes.lock().unwrap();
        let materials = self.factory.inner.materials.lock().unwrap();
        let mesh_factory = self.factory.inner.mesh_factory.lock().unwrap();
        let scene_render = self.resources.get::<SceneRenderer>().unwrap();
        let camera = self.resources.get::<CameraUbo>().unwrap();

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
        );

        // Generate the high-z depth pyramid
        self.generate_hzb(&mut cb, &frame, &canvas);

        // Regenerate froxels
        self.generate_froxels(&mut cb, &frame, &camera);

        // TODO: Perform light clustering

        // Generate draw calls
        self.generate_draw_calls(&mut cb, &frame, &canvas, &scene_render, &camera);

        // Perform the depth prepass
        self.depth_prepass(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &materials,
            &meshes,
            &mesh_factory,
        );

        // Render opaque and alpha masked geometry
        self.render_opaque(
            &mut cb,
            &frame,
            &canvas,
            &camera,
            &scene_render,
            &materials,
            &meshes,
            &mesh_factory,
        );

        // TODO: Render transparent geometry

        // Blit to the surface image
        canvas.blit_to_surface(&mut cb);

        // Submit for rendering
        frame.job = Some(self.ctx.main().submit(Some("primary"), cb));

        // Present the surface image
        canvas.present(&self.ctx, frame.window_size);

        frame
    }

    /// Renders the depth image used to generate the hierarchical-z buffer
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
    ) {
        commands.render_pass(canvas.render_target().hzb_pass(), |pass| {
            if frame_data.object_data.static_dirty(frame_data.frame) {
                info!("Skipping HZB render because static objects are dirty.");
                return;
            }

            scene_render.render_hzb(
                frame_data.frame,
                SceneRenderArgs {
                    camera,
                    pass,
                    mesh_factory,
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

    /// Generates draw calls.
    fn generate_draw_calls<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        scene_render: &'a SceneRenderer,
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
    }

    /// Performs the entire depth prepass.
    fn depth_prepass<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        scene_render: &'a SceneRenderer,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
    ) {
        commands.render_pass(canvas.render_target().depth_prepass(), |pass| {
            scene_render.render_depth_prepass(
                frame_data.frame,
                SceneRenderArgs {
                    pass,
                    camera,
                    mesh_factory,
                    meshes,
                    materials,
                },
            );
        });
    }

    /// Renders opaque geometry
    fn render_opaque<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        scene_render: &'a SceneRenderer,
        materials: &'a ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>,
        meshes: &'a ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>,
        mesh_factory: &'a MeshFactory,
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
                        mesh_factory,
                        meshes,
                        materials,
                    },
                );
            },
        );
    }
}
