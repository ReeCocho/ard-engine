pub mod composite;
pub mod gui;
pub mod mesh_passes;

use std::{
    ops::Div,
    sync::{Arc, Mutex},
};

use crate::{
    camera::{
        CameraUbo, CubeMapInner, DebugDrawing, DebugGui, EntityImage, Lighting, PipelineType,
        Surface,
    },
    context::GraphicsContext,
    factory::Factory,
    renderer::StaticGeometry,
    shader_constants::{FRAMES_IN_FLIGHT, MAX_SHADOW_CASCADES},
    VkBackend,
};

use ard_core::prelude::Disabled;
use ard_ecs::{
    entity::Entity,
    prelude::{ComponentQuery, Queries, Read},
};
use ard_graphics_api::prelude::*;
use ard_math::{Mat4, Vec2};
use ard_render_graph::{
    buffer::{BufferAccessDescriptor, BufferDescriptor, BufferId, BufferUsage},
    graph::{RenderGraph, RenderGraphBuilder, RenderGraphResources},
    image::{ImageAccessDecriptor, ImageDescriptor, ImageId, SizeGroup, SizeGroupId},
    pass::{
        ClearColor, ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, PassDescriptor,
        PassId,
    },
    AccessType, LoadOp, Operations,
};
use ash::vk;
use bytemuck::{Pod, Zeroable};

use self::{
    composite::Composite,
    gui::GuiRender,
    mesh_passes::{
        mesh_pass::{ColorRendering, MeshPassCamera, MeshPassCreateInfo},
        MeshPassId, MeshPasses, MeshPassesBuilder,
    },
};

use super::graph::{RenderGraphContext, RenderPass};

/// Packs together the id's of a pipeline, material, and mesh. Used to sort draw calls.
pub(crate) type DrawKey = u64;

/// Forward plus internals for render graph.
pub(crate) struct ForwardPlus {
    ctx: GraphicsContext,
    factory: Factory,
    surface: Surface,
    static_geo: StaticGeometry,
    debug_drawing: DebugDrawing,
    mesh_passes: MeshPasses,
    gui: GuiRender,
    composite: Composite,
    entity_image_pass: MeshPassId,
    entity_image_copy_pass: PassId,
    shadow_passes: [MeshPassId; MAX_SHADOW_CASCADES],
    passes: Passes,
    /// Size group used by images that the game is rendered to. This may not be the same size as
    /// the surface.
    canvas_size_group: SizeGroupId,
    /// Size group used by the entity image. Should be half the size of `canvas_size_group`.
    entity_image_size_group: SizeGroupId,
    /// Size group that always matches the surface size.
    surface_size_group: SizeGroupId,
    /// Imagine that contains the game scene view.
    scene_image: ImageId,
    /// Final composite imagine to draw to the screen.
    final_image: ImageId,
    /// Buffer that holds the entity image after rendering is complete.
    entity_image_buffer: BufferId,
    entity_image: ImageId,
    frame_data: Vec<FrameData>,
    surface_image_idx: usize,
    work_group_size: u32,
}

/// Per frame data. Must be manually released.
pub(crate) struct FrameData {
    /// Fence indicating rendering is completely finished.
    pub fence: vk::Fence,
    /// Semaphore for main rendering.
    pub main_semaphore: vk::Semaphore,
    /// Indicates that the entity image is ready for read back.
    pub read_back_entity_image: bool,
}

/// Container for Vulkan render passes used in the forward plus renderer.
#[derive(Copy, Clone)]
pub(crate) struct Passes {
    /// Pass which performs depth only rendering of static geometry to an offscreen image.
    pub highz_render: PassId,
    /// Performs depth only rendering of all geometry to minimize fragment overdraw during further
    /// passes.
    pub depth_prepass: PassId,
    /// Draws the color of opaque objects.
    pub opaque_pass: PassId,
    /// Draws the entitie handles of objects.
    pub entity_pass: PassId,
}

/// For convenience.
pub(crate) type GameRendererGraph = RenderGraph<RenderGraphContext<ForwardPlus>>;
pub(crate) type GameRendererGraphRef = Arc<Mutex<GameRendererGraph>>;

impl ForwardPlus {
    pub unsafe fn new_graph(
        ctx: &GraphicsContext,
        surface: &Surface,
        rg_ctx: &mut RenderGraphContext<Self>,
        static_geo: StaticGeometry,
        lighting: &Lighting,
        anisotropy_level: Option<AnisotropyLevel>,
        canvas_size: vk::Extent2D,
        draw_scene: bool,
    ) -> (GameRendererGraphRef, Factory, DebugDrawing, DebugGui, Self) {
        // Create the graph
        let surface_format = surface.0.lock().unwrap().format.format;
        let color_format = vk::Format::R8G8B8A8_UNORM;
        let depth_format =
            pick_depth_format(ctx).expect("unable to find a compatible depth format");

        let mut rg_builder = RenderGraphBuilder::new();

        let canvas_size_group = rg_builder.add_size_group(SizeGroup {
            width: canvas_size.width,
            height: canvas_size.height,
            array_layers: 1,
            mip_levels: 1,
        });

        let entity_image_dims = vk::Extent2D {
            width: canvas_size.width.div(4).max(1),
            height: canvas_size.height.div(4).max(1),
        };

        let entity_image_size_group = rg_builder.add_size_group(SizeGroup {
            width: entity_image_dims.width,
            height: entity_image_dims.height,
            array_layers: 1,
            mip_levels: 1,
        });

        let surface_size_group = {
            let surface = surface.0.lock().unwrap();

            rg_builder.add_size_group(SizeGroup {
                width: surface.resolution.width,
                height: surface.resolution.height,
                array_layers: 1,
                mip_levels: 1,
            })
        };

        let entity_image_buffer = rg_builder.add_buffer(BufferDescriptor {
            size: (entity_image_dims.width * entity_image_dims.height) as u64
                * std::mem::size_of::<Entity>() as u64,
            usage: BufferUsage::ReadBack,
        });

        let mut shadow_size_groups = [SizeGroupId::default(); MAX_SHADOW_CASCADES];
        for i in 0..MAX_SHADOW_CASCADES {
            shadow_size_groups[i] = rg_builder.add_size_group(SizeGroup {
                // TODO: Make this configurable
                width: (4096 / 2_u32.pow(i as u32)).max(2048),
                height: (4096 / 2_u32.pow(i as u32)).max(2048),
                array_layers: 1,
                mip_levels: 1,
            });
        }

        let mut shadow_images = [ImageId::default(); MAX_SHADOW_CASCADES];
        for i in 0..MAX_SHADOW_CASCADES {
            shadow_images[i] = rg_builder.add_image(ImageDescriptor {
                format: depth_format,
                size_group: shadow_size_groups[i],
            });
        }

        let depth_buffer = rg_builder.add_image(ImageDescriptor {
            format: depth_format,
            size_group: canvas_size_group,
        });

        let scene_image = rg_builder.add_image(ImageDescriptor {
            format: color_format,
            size_group: canvas_size_group,
        });

        let entity_image_depth_buffer = rg_builder.add_image(ImageDescriptor {
            format: depth_format,
            size_group: entity_image_size_group,
        });

        let entity_image = rg_builder.add_image(ImageDescriptor {
            format: vk::Format::R32G32_UINT,
            size_group: entity_image_size_group,
        });

        let final_image = rg_builder.add_image(ImageDescriptor {
            format: surface_format,
            size_group: surface_size_group,
        });

        let _begin_recording = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: begin_recording,
        });

        // Create mesh passes
        let mut mp_builder = MeshPassesBuilder::new(ctx, lighting, &mut rg_builder);

        // Shadow passes (one per cascade)
        let mut shadow_passes = [MeshPassId::default(); MAX_SHADOW_CASCADES];
        for (i, pass) in shadow_passes.iter_mut().enumerate() {
            *pass = mp_builder.add_pass(MeshPassCreateInfo {
                toggleable: false,
                size_group: shadow_size_groups[i],
                layers: RenderLayer::ShadowCaster.into(),
                camera: MeshPassCamera::Custom {
                    ubo: CameraUbo::default(),
                },
                highz_culling: false,
                shadow_images: None,
                depth_image: DepthStencilAttachmentDescriptor {
                    image: shadow_images[i],
                    ops: Operations {
                        load: LoadOp::Clear((1.0, 0)),
                        store: true,
                    },
                },
                color_image: None,
                depth_pipeline_type: PipelineType::ShadowPass,
                color_pipeline_type: PipelineType::OpaquePass,
            });
        }

        // Primary mesh pass
        mp_builder.add_pass(MeshPassCreateInfo {
            toggleable: false,
            size_group: canvas_size_group,
            layers: RenderLayerFlags::all(),
            camera: MeshPassCamera::Main,
            highz_culling: true,
            depth_pipeline_type: PipelineType::DepthPrepass,
            color_pipeline_type: PipelineType::OpaquePass,
            shadow_images: Some(shadow_images),
            depth_image: DepthStencilAttachmentDescriptor {
                image: depth_buffer,
                ops: Operations {
                    load: LoadOp::Clear((1.0, 0)),
                    store: false,
                },
            },
            color_image: Some(ColorRendering {
                color_image: ColorAttachmentDescriptor {
                    image: scene_image,
                    ops: Operations {
                        load: LoadOp::Clear(ClearColor::RGBAF32([0.0, 0.0, 0.0, 0.0])),
                        store: true,
                    },
                },
            }),
        });

        // Entity image mesh pass
        let entity_image_pass = mp_builder.add_pass(MeshPassCreateInfo {
            toggleable: true,
            size_group: entity_image_size_group,
            layers: RenderLayerFlags::all(),
            camera: MeshPassCamera::Main,
            highz_culling: true,
            depth_pipeline_type: PipelineType::DepthPrepass,
            color_pipeline_type: PipelineType::EntityImagePass,
            shadow_images: None,
            depth_image: DepthStencilAttachmentDescriptor {
                image: entity_image_depth_buffer,
                ops: Operations {
                    load: LoadOp::Clear((1.0, 0)),
                    store: false,
                },
            },
            color_image: Some(ColorRendering {
                color_image: ColorAttachmentDescriptor {
                    image: entity_image,
                    ops: Operations {
                        load: LoadOp::Clear(ClearColor::RGBAU32([
                            Entity::null().id(),
                            Entity::null().ver(),
                            0,
                            0,
                        ])),
                        store: true,
                    },
                },
            }),
        });

        let mut mesh_passes = mp_builder.build();

        // Copies the scene view onto the final image
        let composite = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            images: vec![ImageAccessDecriptor {
                image: scene_image,
                access: AccessType::Read,
            }],
            color_attachments: vec![ColorAttachmentDescriptor {
                image: final_image,
                ops: Operations {
                    load: LoadOp::Clear(ClearColor::RGBAF32([0.0, 0.0, 0.0, 0.0])),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: Composite::compose,
        });

        // Render pass for GUI rendering
        let gui_pass = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            images: Vec::default(),
            color_attachments: vec![ColorAttachmentDescriptor {
                image: final_image,
                ops: Operations {
                    load: LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: GuiRender::render,
        });

        // Put the final image onto a swapchain image
        let _surface_blit = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: surface_blit,
        });

        // Copy the entity image to the read back buffer.
        let entity_image_copy_pass = rg_builder.add_pass(PassDescriptor::ComputePass {
            toggleable: true,
            code: entity_image_copy,
            images: Vec::default(),
            buffers: vec![BufferAccessDescriptor {
                buffer: entity_image_buffer,
                access: AccessType::ReadWrite,
            }],
        });

        // Submit for presentation
        let _surface_present = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: end_recording,
        });

        let graph = Arc::new(Mutex::new(
            rg_builder
                .build(rg_ctx)
                .expect("unable to create forward plus render graph"),
        ));

        let passes = Passes {
            highz_render: mesh_passes.get_highz_pass_id().unwrap(),
            depth_prepass: mesh_passes.get_depth_prepass_id().unwrap(),
            opaque_pass: mesh_passes.get_opaque_pass_id().unwrap(),
            entity_pass: mesh_passes.get_entity_pass_id().unwrap(),
        };

        mesh_passes.initialize_skybox(match graph.lock().unwrap().get_pass(passes.opaque_pass) {
            RenderPass::Graphics { pass, .. } => *pass,
            _ => panic!("invalid render pass type"),
        });

        // Create the factory
        let factory = Factory::new(
            ctx,
            anisotropy_level,
            &passes,
            &graph,
            mesh_passes.global_pool.layout(),
            mesh_passes.camera_pool.layout(),
        );

        // Create debug drawing utility
        let debug_drawing = DebugDrawing::new(
            ctx,
            mesh_passes.camera_pool.layout(),
            if let RenderPass::Graphics { pass, .. } =
                graph.lock().unwrap().get_pass(passes.opaque_pass)
            {
                *pass
            } else {
                panic!("incorrect pass type")
            },
        );

        // Create gui renderer
        let (gui, debug_gui) = GuiRender::new(
            ctx,
            factory.0.texture_sets.lock().unwrap().layout(),
            match graph.lock().unwrap().get_pass(gui_pass) {
                RenderPass::Graphics { pass, .. } => *pass,
                _ => panic!("incorrect pass type for gui render"),
            },
        );

        // Create composite system
        let composite = Composite::new(
            ctx,
            match graph.lock().unwrap().get_pass(composite) {
                RenderPass::Graphics { pass, .. } => *pass,
                _ => panic!("incorrect pass type for composite pass"),
            },
            draw_scene,
        );

        let mut frame_data = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            frame_data.push(FrameData::new(ctx));
        }

        // Transition the highz image from undefined to transfer src for the culling pass in the first frame
        mesh_passes.transition_highz_images(graph.lock().unwrap().resources());

        let forward_plus = Self {
            ctx: ctx.clone(),
            surface: surface.clone(),
            static_geo,
            factory: factory.clone(),
            debug_drawing: debug_drawing.clone(),
            gui,
            composite,
            entity_image_pass,
            entity_image_copy_pass,
            passes,
            canvas_size_group,
            surface_size_group,
            entity_image_size_group,
            entity_image_buffer,
            entity_image,
            frame_data,
            surface_image_idx: 0,
            work_group_size: ctx.0.properties.limits.max_compute_work_group_size[0],
            mesh_passes,
            shadow_passes,
            scene_image,
            final_image,
        };

        (graph, factory, debug_drawing, debug_gui, forward_plus)
    }

    #[inline]
    pub fn canvas_size_group(&self) -> SizeGroupId {
        self.canvas_size_group
    }

    #[inline]
    pub fn surface_size_group(&self) -> SizeGroupId {
        self.surface_size_group
    }

    #[inline]
    pub fn entity_image_size_group(&self) -> SizeGroupId {
        self.entity_image_size_group
    }

    /// Wait for rendering to complete on the given frame.
    #[inline]
    pub unsafe fn wait(&self, frame: usize) {
        let fence = [self.frame_data[frame].fence];
        self.ctx
            .0
            .device
            .wait_for_fences(&fence, true, u64::MAX)
            .expect("unable to wait on rendering fence");
        self.ctx
            .0
            .device
            .reset_fences(&fence)
            .expect("unable to reset rendering fence");
    }

    #[inline]
    pub fn passes(&self) -> &Passes {
        &self.passes
    }

    #[inline]
    pub fn frames(&self) -> &[FrameData] {
        &self.frame_data
    }

    #[inline]
    pub fn toggle_entity_image_render(
        &mut self,
        enabled: bool,
        frame: usize,
        graph: &mut GameRendererGraph,
    ) {
        if enabled {
            self.frame_data[frame].read_back_entity_image = true;
        }

        self.mesh_passes
            .toggle_pass(self.entity_image_pass, enabled, graph);
        graph.toggle_pass(self.entity_image_copy_pass, enabled);
    }

    #[inline]
    pub fn set_scene_render(&mut self, enabled: bool) {
        self.composite.render_scene = enabled;
    }

    #[inline]
    pub fn set_gui_draw_data(&mut self, frame: usize, draw_data: &imgui::DrawData) {
        self.gui.prepare(frame, draw_data);
    }

    #[inline]
    pub unsafe fn set_skybox_texture(
        &mut self,
        frame: usize,
        texture: &CubeMapInner,
        sampler: vk::Sampler,
    ) {
        self.mesh_passes.draw_sky = true;

        let image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(sampler)
            .build()];

        for pass in &self.mesh_passes.passes {
            let write = [vk::WriteDescriptorSet::builder()
                .dst_set(pass.global_sets[frame])
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_binding(7)
                .image_info(&image_info)
                .build()];

            self.ctx.0.device.update_descriptor_sets(&write, &[]);
        }
    }

    #[inline]
    pub unsafe fn set_irradiance_texture(
        &self,
        frame: usize,
        view: Option<vk::ImageView>,
        sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(view.unwrap_or(self.mesh_passes.black_cube_view))
            .sampler(sampler)
            .build()];

        for pass in &self.mesh_passes.passes {
            let write = [vk::WriteDescriptorSet::builder()
                .dst_set(pass.global_sets[frame])
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_binding(8)
                .image_info(&image_info)
                .build()];

            self.ctx.0.device.update_descriptor_sets(&write, &[]);
        }
    }

    #[inline]
    pub unsafe fn set_radiance_texture(
        &self,
        frame: usize,
        view: Option<vk::ImageView>,
        sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(view.unwrap_or(self.mesh_passes.black_cube_view))
            .sampler(sampler)
            .build()];

        for pass in &self.mesh_passes.passes {
            let write = [vk::WriteDescriptorSet::builder()
                .dst_set(pass.global_sets[frame])
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_binding(9)
                .image_info(&image_info)
                .build()];

            self.ctx.0.device.update_descriptor_sets(&write, &[]);
        }
    }

    #[inline]
    pub fn set_sun_cameras(&mut self, cameras: &[CameraUbo]) {
        assert_eq!(cameras.len(), MAX_SHADOW_CASCADES);

        for i in 0..MAX_SHADOW_CASCADES {
            self.mesh_passes
                .get_pass_mut(self.shadow_passes[i])
                .camera
                .camera = MeshPassCamera::Custom { ubo: cameras[i] };
        }
    }

    #[inline]
    pub fn set_dynamic_geo(
        &mut self,
        queries: &Queries<(
            Entity,
            (Read<Renderable<VkBackend>>, Read<PointLight>, Read<Model>),
            (Read<Disabled>,),
        )>,
    ) {
        self.mesh_passes.dynamic_geo_query =
            Some(queries.make::<(Entity, (Read<Renderable<VkBackend>>, Read<Model>))>());
        for pass in &mut self.mesh_passes.passes {
            pass.dynamic_geo_query = Some(queries.make::<(
                Entity,
                (Read<Renderable<VkBackend>>, Read<Model>),
                Read<Disabled>,
            )>());
        }
    }

    #[inline]
    pub fn set_point_light_query(
        &mut self,
        queries: &Queries<(
            Entity,
            (Read<Renderable<VkBackend>>, Read<PointLight>, Read<Model>),
            (Read<Disabled>,),
        )>,
    ) {
        self.mesh_passes.point_lights_query =
            Some(queries.make::<(Entity, (Read<PointLight>, Read<Model>), (Read<Disabled>,))>());
    }

    #[inline]
    pub fn set_surface_image_idx(&mut self, idx: usize) {
        self.surface_image_idx = idx;
    }

    pub fn read_back_entity_image(
        &mut self,
        frame_idx: usize,
        dst: &mut EntityImage,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) -> bool {
        let frame = &mut self.frame_data[frame_idx];

        if frame.read_back_entity_image {
            let size_group = resources.get_size_group(self.entity_image_size_group);

            // Update dimensions for the destination
            dst.canvas_size = vk::Extent2D {
                width: size_group.width,
                height: size_group.height,
            };

            dst.canvas.resize(
                size_group.width as usize * size_group.height as usize,
                Entity::null(),
            );

            // Copy back the image data
            let buffer = resources
                .get_buffer(self.entity_image_buffer)
                .unwrap()
                .expect_read_back(frame_idx);

            unsafe {
                std::ptr::copy_nonoverlapping(
                    buffer.map().as_ptr() as *const Entity,
                    dst.canvas.as_mut_ptr(),
                    dst.canvas.len(),
                );
            }

            frame.read_back_entity_image = false;
            true
        } else {
            false
        }
    }

    /// Indicates that the canvas has been resized.
    ///
    /// Also updates depth pyramids for high-z culling.
    ///
    /// # Note
    /// External syncronizaton required. Depth pyramids must not be in use when resize occurs.
    #[inline]
    pub fn resize_canvas(
        &mut self,
        resolution: vk::Extent2D,
        entity_image: &mut EntityImage,
        graph: &mut GameRendererGraph,
        ctx: &mut RenderGraphContext<ForwardPlus>,
    ) {
        graph.update_size_group(
            ctx,
            self.canvas_size_group(),
            SizeGroup {
                width: resolution.width,
                height: resolution.height,
                mip_levels: 1,
                array_layers: 1,
            },
        );

        entity_image.resize(resolution);
        let ei_size = entity_image.canvas_size();

        graph.update_size_group(
            ctx,
            self.entity_image_size_group(),
            SizeGroup {
                width: ei_size.width,
                height: ei_size.height,
                mip_levels: 1,
                array_layers: 1,
            },
        );

        let resources = graph.resources_mut();

        for frame in 0..FRAMES_IN_FLIGHT {
            let buffer = resources
                .get_buffer_mut(self.entity_image_buffer)
                .unwrap()
                .expect_read_back_mut(frame);
            unsafe {
                buffer.expand(
                    ei_size.width as usize
                        * ei_size.height as usize
                        * std::mem::size_of::<Entity>(),
                );
            }
        }

        // Update depth pyramids
        for pass in &mut self.mesh_passes.passes {
            // Mark cameras as needing ssbo regen
            for flag in &mut pass.camera.needs_ssbo_regen {
                *flag = true;
            }

            // Resize the depth pyramid
            unsafe {
                pass.regen_depth_pyramids(resources, &mut self.mesh_passes.depth_pyramid_gen);
            }
        }

        unsafe {
            self.mesh_passes.transition_highz_images(resources);
        }
    }
}

impl FrameData {
    unsafe fn new(ctx: &GraphicsContext) -> Self {
        let create_info = vk::FenceCreateInfo::builder()
            .flags(vk::FenceCreateFlags::SIGNALED)
            .build();

        let fence = ctx
            .0
            .device
            .create_fence(&create_info, None)
            .expect("unable to create rendering fence");

        let create_info = vk::SemaphoreCreateInfo::default();
        let main_semaphore = ctx
            .0
            .device
            .create_semaphore(&create_info, None)
            .expect("unable to create main semaphore");

        Self {
            fence,
            main_semaphore,
            read_back_entity_image: false,
        }
    }

    unsafe fn release(self, ctx: &GraphicsContext) {
        ctx.0.device.destroy_semaphore(self.main_semaphore, None);
        ctx.0.device.destroy_fence(self.fence, None);
    }
}

fn begin_recording(
    _ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    _resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let begin_info = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();

    state.mesh_passes.reset();

    unsafe {
        state
            .ctx
            .0
            .device
            .begin_command_buffer(*commands, &begin_info)
            .expect("unable to begin main command buffer");
    }
}

fn entity_image_copy(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;

    // NOTE: Barrier from surface blit is sufficient for the copy here

    // Copy the entity image into the buffer
    unsafe {
        let size_group = resources.get_size_group(state.entity_image_size_group);

        let regions = [vk::BufferImageCopy::builder()
            .image_extent(vk::Extent3D {
                width: size_group.width,
                height: size_group.height,
                depth: 1,
            })
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_array_layer(0)
                    .layer_count(1)
                    .mip_level(0)
                    .build(),
            )
            .build()];

        device.cmd_copy_image_to_buffer(
            *commands,
            resources.get_image(state.entity_image).unwrap().1[frame_idx]
                .image
                .image(),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            resources
                .get_buffer(state.entity_image_buffer)
                .unwrap()
                .expect_read_back(frame_idx)
                .buffer(),
            &regions,
        );
    }

    // Barrier for host readback
    let barrier = [vk::MemoryBarrier::builder()
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::HOST_READ)
        .build()];

    unsafe {
        device.cmd_pipeline_barrier(
            *commands,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::default(),
            &barrier,
            &[],
            &[],
        );
    }
}

fn surface_blit(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;
    let surface = &state.surface.0.lock().expect("mutex poisoned");

    // Transition surface image for transfer
    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(surface.images[state.surface_image_idx])
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .build()];

    unsafe {
        device.cmd_pipeline_barrier(
            *commands,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::default(),
            &[],
            &[],
            &barrier,
        );
    }

    // Perform blit
    let surface_size_group = resources.get_size_group(state.surface_size_group);

    let region = [vk::ImageBlit::builder()
        .src_subresource(
            vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1)
                .build(),
        )
        .src_offsets([
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: surface_size_group.width as i32,
                y: surface_size_group.height as i32,
                z: 1,
            },
        ])
        .dst_subresource(
            vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1)
                .build(),
        )
        .dst_offsets([
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: surface.resolution.width as i32,
                y: surface.resolution.height as i32,
                z: 1,
            },
        ])
        .build()];

    unsafe {
        device.cmd_blit_image(
            *commands,
            resources.get_image(state.final_image).unwrap().1[frame_idx]
                .image
                .image(),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            surface.images[state.surface_image_idx],
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &region,
            vk::Filter::LINEAR,
        );
    }

    // Transition surface image for presentation
    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(surface.images[state.surface_image_idx])
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ)
        .build()];

    unsafe {
        device.cmd_pipeline_barrier(
            *commands,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::DependencyFlags::default(),
            &[],
            &[],
            &barrier,
        );
    }
}

fn end_recording(
    _ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    _resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    state.mesh_passes.draw_sky = false;

    // End commands and submit
    unsafe {
        state
            .ctx
            .0
            .device
            .end_command_buffer(*commands)
            .expect("unable to end main command buffer");
    }
}

impl Drop for ForwardPlus {
    fn drop(&mut self) {
        unsafe {
            for frame in self.frame_data.drain(..) {
                frame.release(&self.ctx);
            }
        }
    }
}

/// Pick a depth format, or return `None` if there isn't one.
pub(crate) fn pick_depth_format(ctx: &GraphicsContext) -> Option<vk::Format> {
    let formats = [
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ];

    for format in formats {
        let props = unsafe {
            ctx.0
                .instance
                .get_physical_device_format_properties(ctx.0.physical_device, format)
        };
        if props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            && props
                .optimal_tiling_features
                .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
        {
            return Some(format);
        }
    }

    None
}
