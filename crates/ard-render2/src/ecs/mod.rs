use ard_log::info;
use ard_math::{Mat4, Vec2, Vec4};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::{
    active::ActiveCamera, froxels::FroxelGenPipeline, target::RenderTarget, ubo::CameraUbo, Camera,
    CameraClearColor,
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
use ard_render_objects::{Model, RenderFlags};
use ard_render_renderers::{
    draw_gen::DrawGenPipeline,
    highz::HzbRenderer,
    scene::{SceneRenderArgs, SceneRenderer},
    shadow::{ShadowRenderArgs, SunShadowsRenderer},
};
use ard_render_si::bindings::Layouts;
use ard_render_textures::factory::TextureFactory;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{canvas::Canvas, factory::Factory, frame::FrameData, RenderPlugin, FRAMES_IN_FLIGHT};

mod camera;
mod factory;
mod scene_render;

pub(crate) struct RenderEcs {
    _layouts: Layouts,
    canvas: Canvas,
    camera: CameraUbo,
    scene_renderer: SceneRenderer,
    sun_shadows_renderer: SunShadowsRenderer,
    lighting: Lighting,
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

        // Create layouts resource
        let layouts = Layouts::new(&ctx);

        // Create the resource factory
        let factory = Factory::new(ctx.clone(), &layouts);

        let hzb_render = HzbRenderer::new(&ctx, &layouts);
        let ao = AmbientOcclusion::new(&ctx, &layouts);
        let draw_gen = DrawGenPipeline::new(&ctx, &layouts);
        let proc_skybox = ProceduralSkyBox::new(&ctx, &layouts);
        let bloom = Bloom::new(&ctx, &layouts, window_size, 6);
        let mut tonemapping = Tonemapping::new(&ctx, &layouts, FRAMES_IN_FLIGHT);

        for frame in 0..FRAMES_IN_FLIGHT {
            tonemapping.bind_bloom(Frame::from(frame), bloom.image());
        }

        (
            Self {
                froxels: FroxelGenPipeline::new(&ctx, &layouts),
                canvas: Canvas::new(
                    &ctx,
                    surface,
                    window_size,
                    plugin.settings.anti_aliasing,
                    plugin.settings.present_mode,
                    &hzb_render,
                    &ao,
                ),
                camera: CameraUbo::new(&ctx, FRAMES_IN_FLIGHT, true, &layouts),
                scene_renderer: SceneRenderer::new(&ctx, &layouts, &draw_gen, FRAMES_IN_FLIGHT),
                sun_shadows_renderer: SunShadowsRenderer::new(
                    &ctx,
                    &layouts,
                    &draw_gen,
                    FRAMES_IN_FLIGHT,
                    4,
                ),
                lighting: Lighting::new(&ctx, &layouts, FRAMES_IN_FLIGHT),
                draw_gen,
                hzb_render,
                ao,
                effect_textures: ImageEffectTextures::new(
                    &ctx,
                    RenderTarget::COLOR_FORMAT,
                    Format::Bgra8Unorm,
                    window_size,
                ),
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
        // Update the canvas size and acquire a new swap chain image
        if self.canvas.resize(
            &self.ctx,
            &self.hzb_render,
            &self.ao,
            &mut self.effect_textures,
            frame.canvas_size,
        ) {
            self.bloom.resize(&self.ctx, frame.canvas_size, 6);

            for frame in 0..FRAMES_IN_FLIGHT {
                self.tonemapping
                    .bind_bloom(Frame::from(frame), self.bloom.image());
            }
        }

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
        rayon::join(
            || {
                self.scene_renderer.upload(
                    frame.frame,
                    &frame.object_data,
                    &meshes,
                    &materials,
                    view_location,
                );
            },
            || {
                self.sun_shadows_renderer.upload(
                    frame.frame,
                    &frame.object_data,
                    &meshes,
                    &materials,
                    view_location,
                );
            },
        );

        // Update sets and bindings
        self.lighting.update_set(frame.frame, &frame.lights);

        self.scene_renderer.update_bindings(
            frame.frame,
            &self.sun_shadows_renderer,
            &self.lighting,
            &frame.object_data,
            &frame.lights,
            self.canvas.hzb(),
            self.canvas.ao(),
            &mesh_factory,
        );

        self.sun_shadows_renderer
            .update_bindings::<FRAMES_IN_FLIGHT>(
                frame.frame,
                &self.lighting,
                &frame.object_data,
                &frame.lights,
                &mesh_factory,
            );

        self.sun_shadows_renderer.update_cascade_views(
            frame.frame,
            &main_camera.camera,
            main_camera.model,
            self.canvas.size(),
            frame.lights.global().sun_direction(),
        );

        // Run the render graph to perform primary rendering
        let mut cb = self.ctx.main().command_buffer();

        ImageEffectsBindImages::new(&self.effect_textures)
            .add(&mut self.bloom)
            .add(&mut self.tonemapping)
            .bind(
                frame.frame,
                self.canvas.render_target().color(),
                self.canvas.render_target().depth(),
                self.canvas.image(),
            );

        // Render the high-z depth image
        self.render_hzb(
            &mut cb,
            &frame,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Generate the high-z depth pyramid
        self.generate_hzb(&mut cb, &frame);

        // Regenerate froxels
        self.generate_froxels(&mut cb, &frame);

        // Perform light clustering
        self.cluster_lights(&mut cb, &frame);

        // Generate draw calls
        self.generate_draw_calls(&mut cb, &frame);

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

        // Render shadows
        Self::render_shadows(
            &mut cb,
            &frame,
            &self.sun_shadows_renderer,
            &materials,
            &meshes,
            &mesh_factory,
            &material_factory,
            &texture_factory,
        );

        // Generate the AO image
        Self::generate_ao_image(&mut cb, &frame, &self.canvas, &self.camera, &self.ao);

        // Render opaque and alpha masked geometry
        Self::render_opaque(
            &mut cb,
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

        // Run image effects and blit to surface
        ImageEffectsRender::new(&self.effect_textures)
            .add(&self.bloom)
            .add(&self.tonemapping)
            .render(
                frame.frame,
                &mut cb,
                self.canvas.render_target().color(),
                self.canvas.image(),
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
        commands.render_pass(self.canvas.render_target().hzb_pass(), |pass| {
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
        });
    }

    #[inline(never)]
    /// Generates the hierarchical-z buffer for use in draw call generation.
    fn generate_hzb<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        self.hzb_render
            .generate(frame_data.frame, commands, self.canvas.hzb());
    }

    #[inline(never)]
    /// Generates camera froxels for light clustering.
    fn generate_froxels<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        if !self.camera.needs_froxel_regen() {
            return;
        }

        info!("Generating camera froxels.");
        commands.compute_pass(|pass| {
            self.froxels.regen(frame_data.frame, pass, &self.camera);
        });
    }

    #[inline(never)]
    // Perform light clustering
    fn cluster_lights<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        self.lighting
            .cluster(commands, frame_data.frame, &self.camera);
    }

    #[inline(never)]
    /// Generates draw calls.
    fn generate_draw_calls<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame_data: &FrameData) {
        let (width, height) = self.canvas.size();
        self.draw_gen.generate(
            frame_data.frame,
            commands,
            self.scene_renderer.draw_gen_sets(),
            &self.camera,
            Vec2::new(width as f32, height as f32),
            false,
        );

        self.sun_shadows_renderer
            .generate_draw_calls(frame_data.frame, commands, &self.draw_gen);
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

    #[inline(never)]
    fn generate_ao_image<'a>(
        commands: &mut CommandBuffer<'a>,
        frame_data: &FrameData,
        canvas: &'a Canvas,
        camera: &'a CameraUbo,
        ao: &'a AmbientOcclusion,
    ) {
        ao.generate(frame_data.frame, commands, canvas.ao(), camera);
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
