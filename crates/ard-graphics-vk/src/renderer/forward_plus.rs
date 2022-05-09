use std::sync::{atomic::Ordering, Arc, Mutex};

use crate::{
    alloc::WriteStorageBuffer,
    camera::{Camera, CameraInner, DebugDrawing, PipelineType, RawPointLight, Surface},
    context::GraphicsContext,
    factory::descriptors::DescriptorPool,
    factory::Factory,
    mesh::VertexLayoutKey,
    renderer::{graph::FRAMES_IN_FLIGHT, StaticGeometry},
    VkBackend,
};

use ard_ecs::prelude::{ComponentQuery, Query, Read};
use ard_graphics_api::prelude::*;
use ard_render_graph::{
    buffer::{BufferAccessDescriptor, BufferDescriptor, BufferId, BufferUsage},
    graph::{RenderGraph, RenderGraphBuilder, RenderGraphResources},
    image::{ImageDescriptor, ImageId, SizeGroup, SizeGroupId},
    pass::{ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, PassDescriptor, PassId},
    AccessType, LoadOp, Operations,
};
use ash::vk;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec4};

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
pub(crate) const POINT_LIGHTS_TABLE_DIMS: (usize, usize, usize) = (32, 16, 16);
pub(crate) const MAX_POINT_LIGHTS_PER_GRID: usize = 256;

/// Forward plus internals for render graph.
pub(crate) struct ForwardPlus {
    ctx: GraphicsContext,
    factory: Factory,
    surface: Surface,
    static_geo: StaticGeometry,
    debug_drawing: DebugDrawing,
    dynamic_geo_query: Option<ComponentQuery<(Read<Renderable<VkBackend>>, Read<Model>)>>,
    point_lights_query: Option<ComponentQuery<(Read<PointLight>, Read<Model>)>>,
    depth_pyramid_gen: DepthPyramidGenerator,
    passes: Passes,
    canvas_size_group: SizeGroupId,
    highz_size_group: SizeGroupId,
    highz_image: ImageId,
    color_image: ImageId,
    draw_calls: BufferId,
    last_draw_calls: GraphBuffer,
    object_info: BufferId,
    input_ids: BufferId,
    output_ids: BufferId,
    point_lights: BufferId,
    point_lights_table: BufferId,
    draw_gen_pipeline_layout: vk::PipelineLayout,
    draw_gen_pipeline: vk::Pipeline,
    point_light_gen_pipeline_layout: vk::PipelineLayout,
    point_light_gen_pipeline: vk::Pipeline,
    /// Descriptor pool for frame global data.
    global_pool: DescriptorPool,
    /// Descriptor pool for draw call generation.
    draw_gen_pool: DescriptorPool,
    /// Descriptor pool for point light culling.
    point_light_gen_pool: DescriptorPool,
    /// Per frame objects used during rendering.
    frame_data: Vec<FrameData>,
    /// Draw keys used during render sorting. One buffer per frame in flight.
    keys: Vec<Vec<(DrawKey, usize)>>,
    object_buffers_expanded: bool,
    surface_image_idx: usize,
    total_objects: usize,
    static_draws: usize,
    dynamic_draws: usize,
    work_group_size: u32,
}

/// Per frame data. Must be manually released.
pub(crate) struct FrameData {
    /// Fence indicating rendering is completely finished.
    pub fence: vk::Fence,
    /// Semaphore for main rendering.
    pub main_semaphore: vk::Semaphore,
    /// Depth pyramid image used for occlusion culling.
    depth_pyramid: DepthPyramid,
    /// Set for point light generation.
    point_light_gen_set: vk::DescriptorSet,
    /// Set for draw call generation data.
    draw_gen_set: vk::DescriptorSet,
    /// Set for global data for rendering.
    global_set: vk::DescriptorSet,
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
    pub _pad: [u32; 2],
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
    light_count:
        [i32; POINT_LIGHTS_TABLE_DIMS.0 * POINT_LIGHTS_TABLE_DIMS.1 * POINT_LIGHTS_TABLE_DIMS.2],
    light_indices: [u32; POINT_LIGHTS_TABLE_DIMS.0
        * POINT_LIGHTS_TABLE_DIMS.1
        * POINT_LIGHTS_TABLE_DIMS.2
        * MAX_POINT_LIGHTS_PER_GRID],
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
        anisotropy_level: Option<AnisotropyLevel>,
        canvas_size: vk::Extent2D,
    ) -> (GameRendererGraphRef, Factory, DebugDrawing, Self) {
        // Create the graph
        let color_format = vk::Format::R8G8B8A8_SRGB;
        let depth_format =
            pick_depth_format(ctx).expect("unable to find a compatible depth format");

        let mut rg_builder = RenderGraphBuilder::new();

        let highz_size_group = rg_builder.add_size_group(SizeGroup {
            width: canvas_size.width,
            height: canvas_size.height,
            array_layers: 1,
            mip_levels: 1,
        });

        let canvas_size_group = rg_builder.add_size_group(SizeGroup {
            width: canvas_size.width,
            height: canvas_size.height,
            array_layers: 1,
            mip_levels: 1,
        });

        let draw_calls = rg_builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<DrawCall>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let last_draw_calls = {
            let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
            for _ in 0..FRAMES_IN_FLIGHT {
                buffers.push(WriteStorageBuffer::new(
                    ctx,
                    (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<DrawCall>()) as usize,
                ));
            }

            GraphBuffer::WriteStorage { buffers }
        };

        let object_info = rg_builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_INFO_BUFFER_CAP * std::mem::size_of::<ObjectInfo>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let input_ids = rg_builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_OBJECT_ID_BUFFER_CAP * std::mem::size_of::<InputObjectId>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let output_ids = rg_builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_OBJECT_ID_BUFFER_CAP * std::mem::size_of::<OutputObjectId>()) as u64,
            usage: BufferUsage::StorageBuffer,
        });

        let point_lights = rg_builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_POINT_LIGHT_CAP * std::mem::size_of::<RawPointLight>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let point_lights_table = rg_builder.add_buffer(BufferDescriptor {
            size: std::mem::size_of::<PointLightTable>() as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let highz_image = rg_builder.add_image(ImageDescriptor {
            format: depth_format,
            size_group: highz_size_group,
        });

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

        let highz_render = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                image: highz_image,
                ops: Operations {
                    load: LoadOp::Clear((1.0, 0)),
                    store: true,
                },
            }),
            buffers: vec![
                BufferAccessDescriptor {
                    buffer: object_info,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: output_ids,
                    access: AccessType::Read,
                },
            ],
            code: highz_render,
        });

        let _highz_generate = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: highz_generate,
        });

        let _prepare_draw_calls = rg_builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: prepare_draw_calls,
        });

        let _generate_draw_calls = rg_builder.add_pass(PassDescriptor::ComputePass {
            toggleable: false,
            images: Vec::default(),
            buffers: vec![
                BufferAccessDescriptor {
                    buffer: object_info,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: input_ids,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: output_ids,
                    access: AccessType::ReadWrite,
                },
                BufferAccessDescriptor {
                    buffer: draw_calls,
                    access: AccessType::ReadWrite,
                },
            ],
            code: generate_draw_calls,
        });

        let _generate_point_lights = rg_builder.add_pass(PassDescriptor::ComputePass {
            toggleable: false,
            images: Vec::default(),
            buffers: vec![
                BufferAccessDescriptor {
                    buffer: point_lights,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: point_lights_table,
                    access: AccessType::ReadWrite,
                },
            ],
            code: generate_point_lights,
        });

        let depth_prepass = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                image: depth_buffer,
                ops: Operations {
                    load: LoadOp::Clear((1.0, 0)),
                    store: true,
                },
            }),
            buffers: vec![
                BufferAccessDescriptor {
                    buffer: object_info,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: output_ids,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: point_lights,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: point_lights_table,
                    access: AccessType::Read,
                },
            ],
            code: depth_prepass,
        });

        let opaque_pass = rg_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: vec![ColorAttachmentDescriptor {
                image: color_image,
                ops: Operations {
                    load: LoadOp::Clear([0.0, 0.0, 0.0, 0.0]),
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                image: depth_buffer,
                ops: Operations {
                    load: LoadOp::Load,
                    store: false,
                },
            }),
            buffers: vec![
                BufferAccessDescriptor {
                    buffer: object_info,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: output_ids,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: point_lights,
                    access: AccessType::Read,
                },
                BufferAccessDescriptor {
                    buffer: point_lights_table,
                    access: AccessType::Read,
                },
            ],
            code: opaque_pass,
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
            highz_render,
            depth_prepass,
            opaque_pass,
        };

        // Create neccesary objects for forward plus rendering
        let mut global_pool = {
            let bindings = [
                // Object info
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Point lights
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Object indices
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Point light clusters
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, FRAMES_IN_FLIGHT)
        };

        let mut point_light_gen_pool = {
            let bindings = [
                // Point lights
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Point light table
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Sampler for high-z image
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, SETS_PER_POOL)
        };

        let mut draw_gen_pool = {
            let bindings = [
                // Object info
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Object IDs
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Draw calls
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Output object indices
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // Sampler for high-z image
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(4)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, SETS_PER_POOL)
        };

        // Create the factory
        let factory = Factory::new(ctx, anisotropy_level, &passes, &graph, global_pool.layout());

        let draw_gen_workgroup_size = ctx.0.properties.limits.max_compute_work_group_size[0];

        // Create debug drawing utility
        let debug_drawing = DebugDrawing::new(
            ctx,
            &factory,
            if let RenderPass::Graphics { pass, .. } = graph.lock().unwrap().get_pass(opaque_pass) {
                *pass
            } else {
                panic!("incorrect pass type")
            },
        );

        // Point light culling
        let point_light_gen_pipeline_layout = {
            let layouts = [
                point_light_gen_pool.layout(),
                factory.0.camera_pool.lock().unwrap().layout(),
            ];

            let push_ranges = [vk::PushConstantRange::builder()
                .offset(0)
                .size(std::mem::size_of::<PointLightGenInfo>() as u32)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&push_ranges)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create point light gen pipeline layout")
        };

        let module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: POINT_LIGHT_GEN_CODE.as_ptr() as *const u32,
                code_size: POINT_LIGHT_GEN_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create point light culling shader module")
        };

        let point_light_gen_pipeline = {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [vk::SpecializationMapEntry::builder()
                .offset(0)
                .size(std::mem::size_of::<u32>())
                .constant_id(0)
                .build()];

            let as_bytes = draw_gen_workgroup_size.to_ne_bytes();

            let specialization_info = vk::SpecializationInfo::builder()
                .map_entries(&map_entries)
                .data(&as_bytes)
                .build();

            let stage_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(module)
                .name(&entry_name)
                .specialization_info(&specialization_info)
                .build();

            let create_info = [vk::ComputePipelineCreateInfo::builder()
                .stage(stage_info)
                .layout(point_light_gen_pipeline_layout)
                .build()];

            ctx.0
                .device
                .create_compute_pipelines(vk::PipelineCache::null(), &create_info, None)
                .expect("Unable to create point light gen pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(module, None);

        // Draw call generation
        let draw_gen_pipeline_layout = {
            let layouts = [
                draw_gen_pool.layout(),
                factory.0.camera_pool.lock().unwrap().layout(),
            ];

            let push_ranges = [vk::PushConstantRange::builder()
                .offset(0)
                .size(std::mem::size_of::<DrawGenInfo>() as u32)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&push_ranges)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create draw gen pipeline layout")
        };

        let module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: DRAW_GEN_CODE.as_ptr() as *const u32,
                code_size: DRAW_GEN_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create object culling shader module")
        };

        let draw_gen_pipeline = {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [vk::SpecializationMapEntry::builder()
                .offset(0)
                .size(std::mem::size_of::<u32>())
                .constant_id(0)
                .build()];

            let as_bytes = draw_gen_workgroup_size.to_ne_bytes();

            let specialization_info = vk::SpecializationInfo::builder()
                .map_entries(&map_entries)
                .data(&as_bytes)
                .build();

            let stage_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(module)
                .name(&entry_name)
                .specialization_info(&specialization_info)
                .build();

            let create_info = [vk::ComputePipelineCreateInfo::builder()
                .stage(stage_info)
                .layout(draw_gen_pipeline_layout)
                .build()];

            ctx.0
                .device
                .create_compute_pipelines(vk::PipelineCache::null(), &create_info, None)
                .expect("Unable to create object culling pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(module, None);

        let mut depth_pyramid_gen = DepthPyramidGenerator::new(ctx);

        let mut frame_data = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            frame_data.push(FrameData::new(
                ctx,
                (canvas_size.width, canvas_size.height),
                &mut depth_pyramid_gen,
                &mut global_pool,
                &mut point_light_gen_pool,
                &mut draw_gen_pool,
            ));
        }

        let forward_plus = Self {
            ctx: ctx.clone(),
            surface: surface.clone(),
            static_geo,
            factory: factory.clone(),
            debug_drawing: debug_drawing.clone(),
            passes,
            dynamic_geo_query: None,
            point_lights_query: None,
            depth_pyramid_gen,
            canvas_size_group,
            highz_size_group,
            highz_image,
            color_image,
            draw_calls,
            last_draw_calls,
            point_lights,
            point_lights_table,
            object_info,
            input_ids,
            output_ids,
            draw_gen_pipeline,
            draw_gen_pipeline_layout,
            point_light_gen_pipeline,
            point_light_gen_pipeline_layout,
            global_pool,
            draw_gen_pool,
            point_light_gen_pool,
            frame_data,
            keys: vec![Vec::default(); FRAMES_IN_FLIGHT],
            object_buffers_expanded: false,
            surface_image_idx: 0,
            total_objects: 0,
            static_draws: 0,
            dynamic_draws: 0,
            work_group_size: ctx.0.properties.limits.max_compute_work_group_size[0],
        };

        (graph, factory, debug_drawing, forward_plus)
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
    pub fn set_dynamic_geo(
        &mut self,
        dynamic_geo: ComponentQuery<(Read<Renderable<VkBackend>>, Read<Model>)>,
    ) {
        self.dynamic_geo_query = Some(dynamic_geo);
    }

    #[inline]
    pub fn set_point_light_query(
        &mut self,
        point_lights: ComponentQuery<(Read<PointLight>, Read<Model>)>,
    ) {
        self.point_lights_query = Some(point_lights);
    }

    #[inline]
    pub fn set_surface_image_idx(&mut self, idx: usize) {
        self.surface_image_idx = idx;
    }
}

impl FrameData {
    unsafe fn new(
        ctx: &GraphicsContext,
        canvas_size: (u32, u32),
        depth_pyramid_gen: &mut DepthPyramidGenerator,
        global_pool: &mut DescriptorPool,
        point_light_gen_pool: &mut DescriptorPool,
        draw_gen_pool: &mut DescriptorPool,
    ) -> Self {
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

        let global_set = global_pool.allocate();

        let point_light_set = point_light_gen_pool.allocate();

        let draw_gen_set = draw_gen_pool.allocate();

        let depth_pyramid = depth_pyramid_gen.allocate(canvas_size.0, canvas_size.1);

        Self {
            fence,
            depth_pyramid,
            main_semaphore,
            global_set,
            point_light_gen_set: point_light_set,
            draw_gen_set,
        }
    }

    unsafe fn release(
        self,
        ctx: &GraphicsContext,
        depth_pyramid_gen: &mut DepthPyramidGenerator,
        global_pool: &mut DescriptorPool,
        point_light_gen_pool: &mut DescriptorPool,
        draw_gen_pool: &mut DescriptorPool,
    ) {
        draw_gen_pool.free(self.draw_gen_set);
        point_light_gen_pool.free(self.point_light_gen_set);
        global_pool.free(self.global_set);
        depth_pyramid_gen.free(self.depth_pyramid);
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

fn highz_render(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame = ctx.frame();
    let device = &state.ctx.0.device;

    let dynamic_geo = state
        .dynamic_geo_query
        .as_ref()
        .expect("no dynamic geometry query provided to graph");
    let static_geo_count = state.static_geo.0.len.load(Ordering::Relaxed);
    let dynamic_geo_count = dynamic_geo.len();
    state.total_objects = static_geo_count + dynamic_geo_count;

    // Update main camera UBO
    let factory = &state.factory;
    let static_geo = &state.static_geo;
    let mut cameras = factory.0.cameras.lock().expect("mutex poisoned");
    let main_camera = cameras.get_mut(factory.main_camera().id).unwrap();
    let canvas_size_group = resources.get_size_group(state.canvas_size_group);

    unsafe {
        main_camera.update(
            frame,
            canvas_size_group.width as f32,
            canvas_size_group.height as f32,
        );
    }

    // Expand buffers to the maximimum size that could be needed and keep track if any of them
    // were expanded.
    state.object_buffers_expanded = unsafe {
        let mut expanded = resources
            .get_buffer_mut(state.object_info)
            .unwrap()
            .expect_write_storage_mut(frame)
            .expand(state.total_objects * std::mem::size_of::<ObjectInfo>())
            .is_some();

        expanded = resources
            .get_buffer_mut(state.input_ids)
            .unwrap()
            .expect_write_storage_mut(frame)
            .expand(state.total_objects * std::mem::size_of::<InputObjectId>())
            .is_some()
            || expanded;

        expanded = resources
            .get_buffer_mut(state.output_ids)
            .unwrap()
            .expect_storage_mut(frame)
            .expand(state.total_objects * std::mem::size_of::<OutputObjectId>())
            .is_some()
            || expanded;

        expanded
    };

    // Expand point light buffer if needed
    let point_lights = state.point_lights_query.as_ref().unwrap();
    let point_light_count = point_lights.len();

    unsafe {
        resources
            .get_buffer_mut(state.point_lights)
            .unwrap()
            .expect_write_storage_mut(frame)
            .expand(point_light_count * std::mem::size_of::<RawPointLight>());
    }

    // Update global set with object info and output ID buffers
    unsafe {
        let global_set = state.frame_data[frame].global_set;

        let object_info = resources
            .get_buffer(state.object_info)
            .unwrap()
            .expect_write_storage(frame);

        let point_lights = resources
            .get_buffer(state.point_lights)
            .unwrap()
            .expect_write_storage(frame);

        let output_idx = resources
            .get_buffer(state.output_ids)
            .unwrap()
            .expect_storage(frame);

        let clusters = resources
            .get_buffer(state.point_lights_table)
            .unwrap()
            .expect_write_storage(frame);

        let input_infos = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(object_info.size())
            .buffer(object_info.buffer())
            .build()];

        let point_light_info = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(point_lights.size())
            .buffer(point_lights.buffer())
            .build()];

        let output_indices = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(output_idx.size())
            .buffer(output_idx.buffer())
            .build()];

        let clusters_info = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(clusters.size())
            .buffer(clusters.buffer())
            .build()];

        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&input_infos)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&point_light_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&output_indices)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&clusters_info)
                .build(),
        ];

        device.update_descriptor_sets(&writes, &[]);
    }

    // We only draw if static geo isn't dirty because if it is dirty the model buffer
    // from last frame will be flushed, invalidating our draws. Also don't draw if the object
    // buffers were expanded because that will also invalidate our draws.
    if !static_geo.0.dirty[frame].load(Ordering::Relaxed) && !state.object_buffers_expanded {
        unsafe {
            render(RenderArgs {
                frame_idx: frame,
                device,
                pipeline_type: PipelineType::HighZRender.idx(),
                commands: *commands,
                main_camera,
                factory,
                global_set: state.frame_data[frame].global_set,
                draw_calls: resources
                    .get_buffer(state.draw_calls)
                    .unwrap()
                    .expect_write_storage(frame)
                    .buffer(),
                keys: &state.keys[frame],
                draw_offset: 0,
                draw_count: state.static_draws,
            });
        }
    }
}

fn highz_generate(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let frame = &mut state.frame_data[frame_idx];

    // Generate the depth pyramid for the frame
    unsafe {
        let depth_image = &resources.get_image(state.highz_image).unwrap().1[frame_idx];

        state.depth_pyramid_gen.gen_pyramid(
            *commands,
            &depth_image.image,
            depth_image.view,
            &frame.depth_pyramid,
        );
    }

    // Now that we're done with the previous frames data, we can swap our draw call and buffer
    std::mem::swap(
        state.last_draw_calls.expect_write_storage_mut(frame_idx),
        resources
            .get_buffer_mut(state.draw_calls)
            .unwrap()
            .expect_write_storage_mut(frame_idx),
    );
}

fn prepare_draw_calls(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    _commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();

    // Clear old draw call keys
    state.keys[frame_idx].clear();

    let static_objects = state.static_geo.0.len.load(Ordering::Relaxed);
    let dynamic_objects = state.dynamic_geo_query.as_ref().unwrap().len();
    let materials = state.factory.0.materials.lock().expect("mutex poisoned");

    let static_draws = state.static_geo.0.batches.len();
    let mut total_draws = static_draws;

    let object_info_buffer = resources
        .get_buffer(state.object_info)
        .unwrap()
        .expect_write_storage(frame_idx);

    let input_id_buffer = resources
        .get_buffer(state.input_ids)
        .unwrap()
        .expect_write_storage(frame_idx);

    // Only need to prepare certain parts of buffers if static geometry has changed or the
    // buffers were recreated
    if state.static_geo.0.dirty[frame_idx].swap(false, Ordering::Relaxed)
        || state.object_buffers_expanded
    {
        let sorted_keys = state
            .static_geo
            .0
            .sorted_keys
            .lock()
            .expect("mutex poisoned");
        let mut cur_offset = 0;

        for (batch_idx, key) in sorted_keys.iter().enumerate() {
            let batch = state.static_geo.0.batches.get(key).unwrap();
            let material = materials.get(batch.material.id).expect("invalid material");

            for i in 0..batch.models.len() {
                unsafe {
                    object_info_buffer.write(
                        cur_offset,
                        ObjectInfo {
                            model: batch.models[i],
                            material_idx: if let Some(idx) = material.material_slot {
                                idx
                            } else {
                                0
                            },
                            textures_idx: if let Some(idx) = material.texture_slot {
                                idx
                            } else {
                                0
                            },
                            _pad: [0; 2],
                        },
                    );

                    input_id_buffer.write(
                        cur_offset,
                        InputObjectId {
                            info_idx: cur_offset as u32,
                            batch_idx: [batch_idx as u32, 0],
                        },
                    );
                }
                cur_offset += 1;
            }
        }

        unsafe {
            object_info_buffer.flush(0, cur_offset * std::mem::size_of::<ObjectInfo>());
            input_id_buffer.flush(0, cur_offset * std::mem::size_of::<InputObjectId>());
        }
    }

    // Write dynamic geometry models
    let object_info_map = object_info_buffer.map();
    let input_id_buffer_map = input_id_buffer.map();
    for (i, (renderable, model)) in state
        .dynamic_geo_query
        .take()
        .unwrap()
        .into_iter()
        .enumerate()
    {
        let info_idx = static_objects + i;
        let material = materials
            .get(renderable.material.id)
            .expect("invalid material");

        // Write model matrix
        unsafe {
            *(object_info_map.as_ptr() as *mut ObjectInfo).add(info_idx) = ObjectInfo {
                model: model.0,
                material_idx: if let Some(idx) = material.material_slot {
                    idx
                } else {
                    0
                },
                textures_idx: if let Some(idx) = material.texture_slot {
                    idx
                } else {
                    0
                },
                _pad: [0; 2],
            };

            // Write object index
            // NOTE: Instead of writing the batch index here, we write the draw key so that we can
            // sort the object IDs here in place and then later determine the batch index. This is
            // fine as long as the size of object used for `batch_idx` is the same as the size used
            // for `DrawKey`.
            *(input_id_buffer_map.as_ptr() as *mut InputObjectId).add(info_idx) = InputObjectId {
                info_idx: info_idx as u32,
                batch_idx: bytemuck::cast(crate::util::make_draw_key(
                    &renderable.material,
                    &renderable.mesh,
                )),
            };
        }
    }

    // Write static batch keys
    let out_keys = &mut state.keys[frame_idx];
    {
        let sorted_keys = state
            .static_geo
            .0
            .sorted_keys
            .lock()
            .expect("mutex poisoned");
        for key in sorted_keys.iter() {
            let batch = state.static_geo.0.batches.get(key).unwrap();
            out_keys.push((*key, batch.models.len()));
        }
    }

    // Write dynmaic keys

    // Sort the dymaic geometry portion of the input ids by the draw key we've written
    let id_slice = unsafe {
        std::slice::from_raw_parts_mut(
            (input_id_buffer_map.as_ptr() as *mut InputObjectId).add(static_objects),
            dynamic_objects,
        )
    };
    id_slice.sort_unstable_by_key(|id| bytemuck::cast::<[u32; 2], DrawKey>(id.batch_idx));

    // Convert draw keys into batch indices
    let mut dynamic_draws = 0;
    let mut cur_key = DrawKey::MAX;
    for id in id_slice {
        // New draw key means new draw
        let batch_as_key = bytemuck::cast(id.batch_idx);
        if batch_as_key != cur_key {
            cur_key = batch_as_key;
            out_keys.push((cur_key, 0));
            dynamic_draws += 1;
        }

        // Update batch index and associated key size
        let batch_idx = static_draws + (dynamic_draws - 1);
        out_keys[batch_idx].1 += 1;
        id.batch_idx[0] = batch_idx as u32;
    }

    // Flush dynamic objects
    if dynamic_objects > 0 {
        unsafe {
            object_info_buffer.flush(
                static_objects * std::mem::size_of::<ObjectInfo>(),
                dynamic_objects * std::mem::size_of::<ObjectInfo>(),
            );
            input_id_buffer.flush(
                static_objects * std::mem::size_of::<InputObjectId>(),
                dynamic_objects * std::mem::size_of::<InputObjectId>(),
            );
        }
    }

    total_draws += dynamic_draws;

    // Resize draw buffers if needed
    let draw_call_buffer = resources
        .get_buffer_mut(state.draw_calls)
        .unwrap()
        .expect_write_storage_mut(frame_idx);

    unsafe {
        draw_call_buffer.expand(total_draws * std::mem::size_of::<DrawCall>());
    }

    state.static_draws = static_draws;
    state.dynamic_draws = dynamic_draws;

    // Write static draws
    let mut cur_object_offset = 0;
    {
        let sorted_keys = state
            .static_geo
            .0
            .sorted_keys
            .lock()
            .expect("mutex poisoned");
        for (i, key) in sorted_keys.iter().enumerate() {
            let batch = state.static_geo.0.batches.get(key).unwrap();

            unsafe {
                draw_call_buffer.write(
                    i,
                    DrawCall {
                        indirect: vk::DrawIndexedIndirectCommand {
                            index_count: batch.mesh.info.index_count as u32,
                            instance_count: 0,
                            first_index: batch.mesh.info.index_block.base(),
                            vertex_offset: batch.mesh.info.vertex_block.base() as i32,
                            first_instance: cur_object_offset as u32,
                        },
                        bounds: batch.mesh.info.bounds,
                    },
                );
            }

            cur_object_offset += batch.models.len();
        }
    }

    // Write dynamic draws
    let meshes = state.factory.0.meshes.lock().expect("mutex poisoned");

    for (i, (key, draw_count)) in out_keys.iter().enumerate().skip(static_draws) {
        let (_, _, mesh, _) = crate::util::from_draw_key(*key);
        let mesh = meshes.get(mesh).expect("invalid mesh");

        unsafe {
            draw_call_buffer.write(
                i,
                DrawCall {
                    indirect: vk::DrawIndexedIndirectCommand {
                        index_count: mesh.index_count as u32,
                        instance_count: 0,
                        first_index: mesh.index_block.base(),
                        vertex_offset: mesh.vertex_block.base() as i32,
                        first_instance: cur_object_offset as u32,
                    },
                    bounds: mesh.bounds,
                },
            );
        }

        cur_object_offset += draw_count;
    }

    unsafe {
        draw_call_buffer.flush(0, total_draws * std::mem::size_of::<DrawCall>());
    }
}

fn generate_draw_calls(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;
    let frame = &mut state.frame_data[frame_idx];

    let factory = &state.factory;
    let cameras = factory.0.cameras.lock().expect("mutex poisoned");
    let main_camera = cameras.get(factory.main_camera().id).unwrap();

    // Update draw generation set
    unsafe {
        let object_info_buffer = resources
            .get_buffer(state.object_info)
            .unwrap()
            .expect_write_storage(frame_idx);
        let input_id_buffer = resources
            .get_buffer(state.input_ids)
            .unwrap()
            .expect_write_storage(frame_idx);
        let draw_call_buffer = resources
            .get_buffer(state.draw_calls)
            .unwrap()
            .expect_write_storage(frame_idx);
        let output_id_buffer = resources
            .get_buffer(state.output_ids)
            .unwrap()
            .expect_storage(frame_idx);

        let input_infos = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(object_info_buffer.size())
            .buffer(object_info_buffer.buffer())
            .build()];

        let input_ids = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(input_id_buffer.size())
            .buffer(input_id_buffer.buffer())
            .build()];

        let draw_calls = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(draw_call_buffer.size())
            .buffer(draw_call_buffer.buffer())
            .build()];

        let output_indices = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(output_id_buffer.size())
            .buffer(output_id_buffer.buffer())
            .build()];

        let depth_img = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(frame.depth_pyramid.view())
            .sampler(state.depth_pyramid_gen.sampler())
            .build()];

        let writes = [
            // For draw gen
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.draw_gen_set)
                .buffer_info(&input_infos)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.draw_gen_set)
                .buffer_info(&input_ids)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.draw_gen_set)
                .buffer_info(&draw_calls)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.draw_gen_set)
                .buffer_info(&output_indices)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_set(frame.draw_gen_set)
                .image_info(&depth_img)
                .build(),
        ];

        device.update_descriptor_sets(&writes, &[]);
    }

    // Compute shader for draw generation must wait on depth pyramid generation
    // NOTE: This also acts as a barrier for point light generation
    let barrier = [vk::MemoryBarrier::builder()
        .src_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        )
        .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
        .build()];

    unsafe {
        device.cmd_pipeline_barrier(
            *commands,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::BY_REGION,
            &barrier,
            &[],
            &[],
        );
    }

    // Set up culling state
    let descriptor_sets = [frame.draw_gen_set, main_camera.set];
    let offsets = [main_camera.ubo.aligned_size() as u32 * frame_idx as u32];

    unsafe {
        device.cmd_bind_pipeline(
            *commands,
            vk::PipelineBindPoint::COMPUTE,
            state.draw_gen_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            *commands,
            vk::PipelineBindPoint::COMPUTE,
            state.draw_gen_pipeline_layout,
            0,
            &descriptor_sets,
            &offsets,
        );
    }

    // Determine the number of groups needed
    let object_count = state.total_objects;
    let group_count = if object_count as u32 % state.work_group_size != 0 {
        (object_count as u32 / state.work_group_size) + 1
    } else {
        object_count as u32 / state.work_group_size
    };

    let size_group = resources.get_size_group(state.highz_size_group);

    let draw_gen_info = [DrawGenInfo {
        object_count: object_count as u32,
        render_area: Vec2::new(size_group.width as f32, size_group.height as f32),
    }];

    unsafe {
        device.cmd_push_constants(
            *commands,
            state.draw_gen_pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytemuck::cast_slice(&draw_gen_info),
        );

        // Dispatch for culling
        device.cmd_dispatch(*commands, group_count, 1, 1);
    }
}

fn generate_point_lights(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;
    let frame = &mut state.frame_data[frame_idx];

    let factory = &state.factory;
    let cameras = factory.0.cameras.lock().expect("mutex poisoned");
    let main_camera = cameras.get(factory.main_camera().id).unwrap();

    // Write point lights
    let point_lights = state.point_lights_query.take().unwrap();
    let point_light_count = point_lights.len();

    // Reset light counts
    let point_lights_table_buffer = resources
        .get_buffer_mut(state.point_lights_table)
        .unwrap()
        .expect_write_storage_mut(frame_idx);

    unsafe {
        point_lights_table_buffer.write_slice(
            0,
            &[0 as i32;
                POINT_LIGHTS_TABLE_DIMS.0 * POINT_LIGHTS_TABLE_DIMS.1 * POINT_LIGHTS_TABLE_DIMS.2],
        );
        point_lights_table_buffer.flush(
            0,
            POINT_LIGHTS_TABLE_DIMS.0
                * POINT_LIGHTS_TABLE_DIMS.1
                * POINT_LIGHTS_TABLE_DIMS.2
                * std::mem::size_of::<i32>(),
        );
    }

    let point_lights_buffer = resources
        .get_buffer_mut(state.point_lights)
        .unwrap()
        .expect_write_storage_mut(frame_idx);

    // Write lights into buffer
    for (i, (light, model)) in point_lights.into_iter().enumerate() {
        unsafe {
            let position = model.0.col(3);
            point_lights_buffer.write(
                i,
                RawPointLight {
                    color_intensity: Vec4::new(
                        light.color.x,
                        light.color.y,
                        light.color.z,
                        light.intensity,
                    ),
                    position_range: Vec4::new(position.x, position.y, position.z, light.radius),
                },
            );
        }
    }

    unsafe {
        point_lights_buffer.flush(0, point_light_count * std::mem::size_of::<RawPointLight>());
    }

    // Update point light generation set
    unsafe {
        let point_light_buffer = resources
            .get_buffer(state.point_lights)
            .unwrap()
            .expect_write_storage(frame_idx);

        let point_light_table_buffer = resources
            .get_buffer(state.point_lights_table)
            .unwrap()
            .expect_write_storage(frame_idx);

        let point_lights = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(point_light_buffer.size())
            .buffer(point_light_buffer.buffer())
            .build()];

        let point_light_table = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(point_light_table_buffer.size())
            .buffer(point_light_table_buffer.buffer())
            .build()];

        let depth_img = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(frame.depth_pyramid.view())
            .sampler(state.depth_pyramid_gen.sampler())
            .build()];

        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.point_light_gen_set)
                .buffer_info(&point_lights)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(frame.point_light_gen_set)
                .buffer_info(&point_light_table)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_set(frame.point_light_gen_set)
                .image_info(&depth_img)
                .build(),
        ];

        device.update_descriptor_sets(&writes, &[]);
    }

    // Set up culling state
    let descriptor_sets = [frame.point_light_gen_set, main_camera.set];
    let offsets = [main_camera.ubo.aligned_size() as u32 * frame_idx as u32];

    unsafe {
        device.cmd_bind_pipeline(
            *commands,
            vk::PipelineBindPoint::COMPUTE,
            state.point_light_gen_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            *commands,
            vk::PipelineBindPoint::COMPUTE,
            state.point_light_gen_pipeline_layout,
            0,
            &descriptor_sets,
            &offsets,
        );
    }

    // Determine the number of groups needed
    let group_count = if point_light_count as u32 % state.work_group_size != 0 {
        (point_light_count as u32 / state.work_group_size) + 1
    } else {
        point_light_count as u32 / state.work_group_size
    };

    let size_group = resources.get_size_group(state.highz_size_group);

    let light_gen_info = [PointLightGenInfo {
        light_count: point_light_count as u32,
        render_area: Vec2::new(size_group.width as f32, size_group.height as f32),
    }];

    unsafe {
        device.cmd_push_constants(
            *commands,
            state.draw_gen_pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytemuck::cast_slice(&light_gen_info),
        );

        // Dispatch for culling
        device.cmd_dispatch(*commands, group_count, 1, 1);
    }
}

fn depth_prepass(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;
    let frame = &mut state.frame_data[frame_idx];

    let barrier = [vk::MemoryBarrier::builder()
        .src_access_mask(vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ)
        .dst_access_mask(vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ)
        .build()];

    unsafe {
        device.cmd_pipeline_barrier(
            *commands,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::DependencyFlags::BY_REGION,
            &barrier,
            &[],
            &[],
        );
    }

    let factory = &state.factory;
    let mut cameras = factory.0.cameras.lock().expect("mutex poisoned");
    let main_camera = cameras.get_mut(factory.main_camera().id).unwrap();

    unsafe {
        main_camera.update(frame_idx, 1280.0, 720.0);
    }

    unsafe {
        render(RenderArgs {
            frame_idx,
            device,
            pipeline_type: PipelineType::DepthPrepass.idx(),
            commands: *commands,
            main_camera,
            factory,
            global_set: frame.global_set,
            draw_calls: resources
                .get_buffer(state.draw_calls)
                .unwrap()
                .expect_write_storage(frame_idx)
                .buffer(),
            keys: &state.keys[frame_idx],
            draw_offset: 0,
            draw_count: state.static_draws + state.dynamic_draws,
        });
    }
}

fn opaque_pass(
    ctx: &mut RenderGraphContext<ForwardPlus>,
    state: &mut ForwardPlus,
    commands: &vk::CommandBuffer,
    _pass: &mut RenderPass<ForwardPlus>,
    resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
) {
    let frame_idx = ctx.frame();
    let device = &state.ctx.0.device;
    let frame = &mut state.frame_data[frame_idx];

    let factory = &state.factory;
    let cameras = factory.0.cameras.lock().expect("mutex poisoned");
    let main_camera = cameras.get(factory.main_camera().id).unwrap();

    unsafe {
        // Render geometry
        render(RenderArgs {
            frame_idx,
            device,
            pipeline_type: PipelineType::OpaquePass.idx(),
            commands: *commands,
            main_camera,
            factory,
            global_set: frame.global_set,
            draw_calls: resources
                .get_buffer(state.draw_calls)
                .unwrap()
                .expect_write_storage(frame_idx)
                .buffer(),
            keys: &state.keys[frame_idx],
            draw_offset: 0,
            draw_count: state.static_draws + state.dynamic_draws,
        });

        // Render debug objects
        let canvas_size = resources.get_size_group(state.canvas_size_group);

        state.debug_drawing.0.render(
            *commands,
            frame_idx,
            main_camera,
            (canvas_size.width, canvas_size.height),
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

struct RenderArgs<'a> {
    frame_idx: usize,
    device: &'a ash::Device,
    pipeline_type: usize,
    commands: vk::CommandBuffer,
    main_camera: &'a CameraInner,
    factory: &'a Factory,
    global_set: vk::DescriptorSet,
    draw_calls: vk::Buffer,
    draw_offset: usize,
    draw_count: usize,
    keys: &'a [(DrawKey, usize)],
}

#[allow(clippy::too_many_arguments)]
unsafe fn render(args: RenderArgs) {
    let pipelines = args.factory.0.pipelines.lock().expect("mutex poisoned");
    let mut mesh_buffers = args.factory.0.mesh_buffers.lock().expect("mutex poisoned");
    let meshes = args.factory.0.meshes.lock().expect("mutex poisoned");
    let mut material_buffers = args
        .factory
        .0
        .material_buffers
        .lock()
        .expect("mutex poisoned");

    let mut last_pipeline = u32::MAX;
    let mut _last_material = u32::MAX;
    let mut last_mesh = u32::MAX;
    let mut last_vertex_layout = VertexLayoutKey::MAX;
    let mut last_ubo_size = u64::MAX;

    let mut sub_offset = 0;
    let mut sub_count = 0;
    let mut needs_draw = false;

    // Bind global sets
    let sets = [
        args.global_set,
        args.factory
            .0
            .texture_sets
            .lock()
            .expect("mutex poisoned")
            .get_set(args.frame_idx),
        args.main_camera.set,
    ];

    let offsets = [args.main_camera.ubo.aligned_size() as u32 * args.frame_idx as u32];

    args.device.cmd_bind_descriptor_sets(
        args.commands,
        vk::PipelineBindPoint::GRAPHICS,
        args.factory.0.layouts.opaque_pipeline_layout,
        0,
        &sets,
        &offsets,
    );

    // Bind index buffers
    {
        let idx_buffer = mesh_buffers.get_index_buffer();
        args.device.cmd_bind_index_buffer(
            args.commands,
            idx_buffer.buffer(),
            0,
            vk::IndexType::UINT32,
        );
    }

    for (i, (key, _)) in args.keys[args.draw_offset..(args.draw_offset + args.draw_count)]
        .iter()
        .enumerate()
    {
        let (pipeline_id, vertex_layout, mesh_id, _) = crate::util::from_draw_key(*key);

        // Bind pipeline
        if last_pipeline != pipeline_id {
            let pipeline = pipelines.get(pipeline_id).expect("invalid pipeline");
            args.device.cmd_bind_pipeline(
                args.commands,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipelines[args.pipeline_type],
            );

            // Bind material data if needed
            if pipeline.inputs.ubo_size != last_ubo_size {
                let sets = [material_buffers
                    .get_set(pipeline.inputs.ubo_size, args.frame_idx)
                    .set];

                args.device.cmd_bind_descriptor_sets(
                    args.commands,
                    vk::PipelineBindPoint::GRAPHICS,
                    args.factory.0.layouts.opaque_pipeline_layout,
                    3,
                    &sets,
                    &[],
                );

                last_ubo_size = pipeline.inputs.ubo_size;
            }

            last_pipeline = pipeline_id;
            needs_draw = true;
        }

        // Bind vertex buffers
        if last_vertex_layout != vertex_layout {
            last_vertex_layout = vertex_layout;
            let vertex_layout = crate::util::from_layout_key(vertex_layout);
            let vbuffer = mesh_buffers.get_vertex_buffer(&vertex_layout);
            vbuffer.bind(args.device, args.commands, &vertex_layout);
            needs_draw = true;
        }

        // Check if current mesh is valid
        if last_mesh != mesh_id {
            let mesh = meshes.get(mesh_id).expect("invalid mesh");
            if !mesh.ready {
                needs_draw = true;
                continue;
            }
            last_mesh = mesh_id;
        }

        // Draw if needed
        if needs_draw {
            if sub_count > 0 {
                args.device.cmd_draw_indexed_indirect(
                    args.commands,
                    args.draw_calls,
                    (sub_offset * std::mem::size_of::<DrawCall>()) as u64,
                    sub_count,
                    std::mem::size_of::<DrawCall>() as u32,
                );
            }

            sub_count = 0;
            sub_offset = i;
            needs_draw = false;
        }

        // New draw
        sub_count += 1;
    }

    // Handle final draw
    if sub_count > 0 {
        args.device.cmd_draw_indexed_indirect(
            args.commands,
            args.draw_calls,
            (sub_offset * std::mem::size_of::<DrawCall>()) as u64,
            sub_count,
            std::mem::size_of::<DrawCall>() as u32,
        );
    }
}

impl Drop for ForwardPlus {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .device
                .destroy_pipeline_layout(self.draw_gen_pipeline_layout, None);

            self.ctx
                .0
                .device
                .destroy_pipeline(self.draw_gen_pipeline, None);

            self.ctx
                .0
                .device
                .destroy_pipeline_layout(self.point_light_gen_pipeline_layout, None);

            self.ctx
                .0
                .device
                .destroy_pipeline(self.point_light_gen_pipeline, None);

            for frame in self.frame_data.drain(..) {
                frame.release(
                    &self.ctx,
                    &mut self.depth_pyramid_gen,
                    &mut self.global_pool,
                    &mut self.point_light_gen_pool,
                    &mut self.draw_gen_pool,
                );
            }
        }
    }
}

/// Pick a depth format, or return `None` if there isn't one.
fn pick_depth_format(ctx: &GraphicsContext) -> Option<vk::Format> {
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

const DRAW_GEN_CODE: &[u8] = include_bytes!("draw_gen.comp.spv");

const POINT_LIGHT_GEN_CODE: &[u8] = include_bytes!("point_light_gen.comp.spv");
