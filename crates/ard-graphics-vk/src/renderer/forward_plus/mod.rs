pub mod gui;
pub mod mesh_passes;

use std::sync::{atomic::Ordering, Arc, Mutex};

use crate::{
    alloc::WriteStorageBuffer,
    camera::{
        Camera, CameraInner, CameraUbo, CubeMapInner, DebugDrawing, DebugGui, Lighting,
        PipelineType, RawPointLight, Surface, TextureInner,
    },
    context::GraphicsContext,
    factory::descriptors::DescriptorPool,
    factory::Factory,
    mesh::VertexLayoutKey,
    renderer::StaticGeometry,
    shader_constants::{
        FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS, MAX_POINT_LIGHTS_PER_FROXEL, MAX_SHADOW_CASCADES,
    },
    VkBackend,
};

use ard_ecs::prelude::{ComponentQuery, Queries, Query, Read};
use ard_graphics_api::prelude::*;
use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ard_render_graph::{
    buffer::{BufferAccessDescriptor, BufferDescriptor, BufferId, BufferUsage},
    graph::{RenderGraph, RenderGraphBuilder, RenderGraphResources},
    image::{ImageDescriptor, ImageId, SizeGroup, SizeGroupId},
    pass::{ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, PassDescriptor, PassId},
    AccessType, LoadOp, Operations,
};
use ash::vk;
use bytemuck::{Pod, Zeroable};

use self::{
    gui::GuiRender,
    mesh_passes::{
        mesh_pass::{ColorRendering, MeshPassCamera, MeshPassCreateInfo},
        MeshPassId, MeshPasses, MeshPassesBuilder,
    },
};

use super::{
    depth_pyramid::{DepthPyramid, DepthPyramidGenerator},
    graph::{GraphBuffer, RenderGraphContext, RenderPass},
};

/// Packs together the id's of a pipeline, material, and mesh. Used to sort draw calls.
pub(crate) type DrawKey = u64;

pub(crate) const DEFAULT_INFO_BUFFER_CAP: usize = 1;
pub(crate) const DEFAULT_OBJECT_ID_BUFFER_CAP: usize = 1;
pub(crate) const DEFAULT_DRAW_CALL_CAP: usize = 1;
pub(crate) const DEFAULT_POINT_LIGHT_CAP: usize = 1;
pub(crate) const SETS_PER_POOL: usize = FRAMES_IN_FLIGHT;

/// Forward plus internals for render graph.
pub(crate) struct ForwardPlus {
    ctx: GraphicsContext,
    factory: Factory,
    surface: Surface,
    static_geo: StaticGeometry,
    debug_drawing: DebugDrawing,
    mesh_passes: MeshPasses,
    gui: GuiRender,
    shadow_passes: [MeshPassId; MAX_SHADOW_CASCADES],
    passes: Passes,
    canvas_size_group: SizeGroupId,
    /// Final color image that should be presented.
    color_image: ImageId,
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
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct DrawCall {
    /// Vulkan indirect command.
    pub indirect: vk::DrawIndexedIndirectCommand,
    /// Object bounds of the mesh for this draw call. Used for culling.
    pub bounds: ObjectBounds,
}

unsafe impl Zeroable for DrawCall {}
unsafe impl Pod for DrawCall {}

#[repr(C)]
#[derive(Copy, Clone)]
struct DrawGenInfo {
    render_area: Vec2,
    object_count: u32,
}

unsafe impl Zeroable for DrawGenInfo {}
unsafe impl Pod for DrawGenInfo {}

#[repr(C)]
#[derive(Copy, Clone)]
struct PointLightGenInfo {
    render_area: Vec2,
    light_count: u32,
}

unsafe impl Zeroable for PointLightGenInfo {}
unsafe impl Pod for PointLightGenInfo {}

/// Each object drawn by the renderer is given an object id. `info_idx` points to the info
/// in the ubfi buffer for the object. `batch_idx` points into the batch buffer that the
/// object should be drawn with.
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct InputObjectId {
    pub info_idx: u32,
    /// ## Note
    /// You might be wondering why this is an array. Well, in order to generate dynamic draw calls
    /// we need to sort all the objects by their draw key and then compact duplicates into single
    /// draws. In order to do this, all the objects must know what their batch index is "before we
    /// actually generate them" (this is mostly for performance reasons). With static objects it
    /// isn't an issue because they are already sorted. For dynamic objects we must sort them
    /// ourselves. To do this, we use this field to hold the draw key. Since the draw key is a
    /// 64-bit number, we need two u32 fields to hold it.
    pub batch_idx: [u32; 2],
}

unsafe impl Zeroable for InputObjectId {}
unsafe impl Pod for InputObjectId {}

pub(crate) type OutputObjectId = u32;

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct ObjectInfo {
    pub model: Mat4,
    pub material_idx: u32,
    pub textures_idx: u32,
    pub _pad: Vec2,
}

unsafe impl Zeroable for ObjectInfo {}
unsafe impl Pod for ObjectInfo {}

/// The render area is partitioned into a grid. Each grid contains a list of point lights that
/// influence it. Shaders determine which grid they are in to consider only the lights that
/// influence their current fragment.
#[repr(C)]
pub(crate) struct PointLightTable {
    /// NOTE: Light counts must be a signed integer because GLSL has no `atomicSub`. So, we must
    /// simulate it by adding a negative number which means the values must be signed.
    light_count: [i32; FROXEL_TABLE_DIMS.0 * FROXEL_TABLE_DIMS.1 * FROXEL_TABLE_DIMS.2],
    light_indices: [u32; FROXEL_TABLE_DIMS.0
        * FROXEL_TABLE_DIMS.1
        * FROXEL_TABLE_DIMS.2
        * MAX_POINT_LIGHTS_PER_FROXEL],
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
    ) -> (GameRendererGraphRef, Factory, DebugDrawing, DebugGui, Self) {
        // Create the graph
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

        let mut shadow_size_groups = [SizeGroupId::default(); MAX_SHADOW_CASCADES];
        for i in 0..MAX_SHADOW_CASCADES {
            shadow_size_groups[i] = rg_builder.add_size_group(SizeGroup {
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

        let color_image = rg_builder.add_image(ImageDescriptor {
            format: color_format,
            size_group: canvas_size_group,
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
            });
        }

        // Primary mesh pass
        mp_builder.add_pass(MeshPassCreateInfo {
            size_group: canvas_size_group,
            layers: RenderLayerFlags::all(),
            camera: MeshPassCamera::Main,
            highz_culling: true,
            depth_pipeline_type: PipelineType::HighZRender,
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
                    image: color_image,
                    ops: Operations {
                        load: LoadOp::Clear([0.0, 0.0, 0.0, 0.0]),
                        store: true,
                    },
                },
            }),
        });

        let mut mesh_passes = mp_builder.build();

        // Render pass for GUI rendering
        let gui_pass = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            images: Vec::default(),
            color_attachments: vec![ColorAttachmentDescriptor {
                image: color_image,
                ops: Operations {
                    load: LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: GuiRender::render,
        });

        let _surface_blit = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: surface_blit,
        });

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

        let mut frame_data = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            frame_data.push(FrameData::new(ctx, (canvas_size.width, canvas_size.height)));
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
            passes,
            canvas_size_group,
            frame_data,
            surface_image_idx: 0,
            work_group_size: ctx.0.properties.limits.max_compute_work_group_size[0],
            mesh_passes,
            shadow_passes,
            color_image,
        };

        (graph, factory, debug_drawing, debug_gui, forward_plus)
    }

    #[inline]
    pub fn canvas_size_group(&self) -> SizeGroupId {
        self.canvas_size_group
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
        texture: &CubeMapInner,
        sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
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
        texture: &CubeMapInner,
        sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
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
        queries: &Queries<(Read<Renderable<VkBackend>>, Read<PointLight>, Read<Model>)>,
    ) {
        self.mesh_passes.dynamic_geo_query =
            Some(queries.make::<(Read<Renderable<VkBackend>>, Read<Model>)>());
        for pass in &mut self.mesh_passes.passes {
            pass.dynamic_geo_query = Some(queries.make::<(Read<Renderable<VkBackend>>,)>());
        }
    }

    #[inline]
    pub fn set_point_light_query(
        &mut self,
        point_lights: ComponentQuery<(Read<PointLight>, Read<Model>)>,
    ) {
        self.mesh_passes.point_lights_query = Some(point_lights);
    }

    #[inline]
    pub fn set_surface_image_idx(&mut self, idx: usize) {
        self.surface_image_idx = idx;
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
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
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
    unsafe fn new(ctx: &GraphicsContext, canvas_size: (u32, u32)) -> Self {
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

    unsafe {
        state
            .ctx
            .0
            .device
            .begin_command_buffer(*commands, &begin_info)
            .expect("unable to begin main command buffer");
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
    let canvas_size_group = resources.get_size_group(state.canvas_size_group);

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
                x: canvas_size_group.width as i32,
                y: canvas_size_group.height as i32,
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
            resources.get_image(state.color_image).unwrap().1[frame_idx]
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

const DRAW_GEN_CODE: &[u8] = include_bytes!("../draw_gen.comp.spv");

const POINT_LIGHT_GEN_CODE: &[u8] = include_bytes!("../point_light_gen.comp.spv");

const CLUSTER_GEN_CODE: &[u8] = include_bytes!("../cluster_gen.comp.spv");
