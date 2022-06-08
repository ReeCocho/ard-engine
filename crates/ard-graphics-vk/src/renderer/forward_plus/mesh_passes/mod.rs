pub mod mesh_pass;

use rayon::prelude::*;
use std::sync::atomic::Ordering;

use self::mesh_pass::{MeshPass, MeshPassCreateInfo};

use super::{DrawKey, ForwardPlus};
use crate::{
    alloc::{Buffer, Image, ImageCreateInfo},
    camera::{
        depth_pyramid::DepthPyramidGenerator, descriptors::DescriptorPool, graph::RenderPass,
        GraphicsContext, Lighting, RawPointLight,
    },
    prelude::graph::RenderGraphContext,
    shader_constants::{
        FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS, MAX_POINT_LIGHTS_PER_FROXEL, MAX_SHADOW_CASCADES,
    },
};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::{Mat4, Vec2, Vec4};
use ard_render_graph::{
    buffer::{BufferAccessDescriptor, BufferDescriptor, BufferId, BufferUsage},
    graph::{RenderGraphBuilder, RenderGraphResources},
    image::ImageAccessDecriptor,
    pass::{ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, PassDescriptor, PassId},
    AccessType, LoadOp, Operations,
};
use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::VkBackend;

pub(crate) const DEFAULT_INFO_BUFFER_CAP: usize = 1;
pub(crate) const DEFAULT_POINT_LIGHT_CAP: usize = 1;
pub(crate) const SETS_PER_POOL: usize = FRAMES_IN_FLIGHT * 4;

/// Container of all mesh passes in the Forward+ render graph.
pub(crate) struct MeshPasses {
    pub ctx: GraphicsContext,
    pub passes: Vec<MeshPass>,
    /// List of mesh passes that care about each stage.
    pub stages: [Vec<usize>; MeshPassStage::count()],
    /// Current stage we are in.
    pub active_stage: MeshPassStage,
    /// Index of the currently active mesh pass.
    pub active_pass: usize,
    /// Total number of objects that can be rendered.
    pub total_objects: usize,
    /// Flag indicating if the skybox should be drawn.
    pub draw_sky: bool,
    /// Dynamic geometry query to look through when rendering.
    pub dynamic_geo_query: Option<ComponentQuery<(Read<Renderable<VkBackend>>, Read<Model>)>>,
    /// Query of point lights to read through.
    pub point_lights_query: Option<ComponentQuery<(Read<PointLight>, Read<Model>)>>,
    pub point_light_count: usize,
    /// Flag indicating that object buffers were expanded and thus need to be updated.
    pub object_buffers_expanded: bool,
    /// Used to create depth mip chains for high-z culling.
    pub depth_pyramid_gen: DepthPyramidGenerator,
    /// Sampler used to sample shadow maps.
    pub shadow_sampler: vk::Sampler,
    /// Sampler used when sampling the poisson disk texture.
    pub poisson_disk_sampler: vk::Sampler,
    /// Descriptor pool for frame global data.
    pub global_pool: DescriptorPool,
    /// Descriptor pool for draw call generation.
    pub draw_gen_pool: DescriptorPool,
    /// Descriptor pool for point light clustering.
    pub light_clustering_pool: DescriptorPool,
    /// Descriptor pool for cameras.
    pub camera_pool: DescriptorPool,
    /// Contains a description of every object to possibly render.
    pub object_info: BufferId,
    /// Contains all point lights in the scene.
    pub point_lights: BufferId,
    // Draw call generation pipeline.
    pub draw_gen_pipeline_layout: vk::PipelineLayout,
    pub draw_gen_pipeline: vk::Pipeline,
    pub draw_gen_no_highz_pipeline: vk::Pipeline,
    // Point light clustering pipeline.
    pub point_light_gen_pipeline_layout: vk::PipelineLayout,
    pub point_light_gen_pipeline: vk::Pipeline,
    // Camera cluster generation pipeline.
    pub cluster_gen_pipeline_layout: vk::PipelineLayout,
    pub cluster_gen_pipeline: vk::Pipeline,
    // Skybox rendering pipeline.
    pub skybox_pipeline_layout: vk::PipelineLayout,
    pub skybox_pipeline: vk::Pipeline,
    // Poisson disk image info.
    pub poisson_disk: Image,
    pub poisson_disk_view: vk::ImageView,
    // BRDF look up texture.
    pub brdf_lut_sampler: vk::Sampler,
    pub brdf_lut: Image,
    pub brdf_lut_view: vk::ImageView,
}

#[derive(Debug, Copy, Clone, Default)]
pub(crate) struct MeshPassId(usize);

pub(crate) struct MeshPassesBuilder<'a> {
    passes: MeshPasses,
    ctx: GraphicsContext,
    lighting: &'a Lighting,
    builder: &'a mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum MeshPassStage {
    CameraSetup,
    HighZRender,
    HighZGenerate,
    GenerateDrawCalls,
    ClusterLights,
    DepthPrepass,
    OpaquePass,
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

impl<'a> MeshPassesBuilder<'a> {
    pub fn new(
        ctx: &GraphicsContext,
        lighting: &'a Lighting,
        builder: &'a mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>,
    ) -> Self {
        let object_info = builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_INFO_BUFFER_CAP * std::mem::size_of::<ObjectInfo>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let point_lights = builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_POINT_LIGHT_CAP * std::mem::size_of::<RawPointLight>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let global_pool = unsafe {
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
                // Lighting UBO
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(4)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Shadow maps
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(5)
                    .descriptor_count(MAX_SHADOW_CASCADES as u32)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Poisson disk
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(6)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Skybox
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(7)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Skybox irradiance
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(8)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Skybox radiance
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(9)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // IBL BRDF LUT
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(10)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, FRAMES_IN_FLIGHT)
        };

        let light_clustering_pool = unsafe {
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

        let draw_gen_pool = unsafe {
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

        let camera_pool = unsafe {
            let bindings = [
                // Camera UBO
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .stage_flags(vk::ShaderStageFlags::ALL)
                    .build(),
                // Camera cluster froxels
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER_DYNAMIC)
                    .stage_flags(vk::ShaderStageFlags::ALL)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(
                ctx,
                &layout_create_info,
                FRAMES_IN_FLIGHT * VkBackend::MAX_CAMERA,
            )
        };

        // Point light culling
        let point_light_gen_pipeline_layout = unsafe {
            let layouts = [light_clustering_pool.layout(), camera_pool.layout()];

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

        let module = unsafe {
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

        let point_light_gen_pipeline = unsafe {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [
                vk::SpecializationMapEntry::builder()
                    .offset(0)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(0)
                    .build(),
                vk::SpecializationMapEntry::builder()
                    .offset(std::mem::size_of::<u32>() as u32)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(1)
                    .build(),
                vk::SpecializationMapEntry::builder()
                    .offset(2 * std::mem::size_of::<u32>() as u32)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(2)
                    .build(),
            ];

            let table_dims = [1u32, 1, 1];

            let as_bytes = bytemuck::bytes_of(&table_dims);

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

        unsafe {
            ctx.0.device.destroy_shader_module(module, None);
        }

        // Draw call generation
        let draw_gen_pipeline_layout = unsafe {
            let layouts = [draw_gen_pool.layout(), camera_pool.layout()];

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

        let module = unsafe {
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

        let draw_gen_pipeline = unsafe {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [vk::SpecializationMapEntry::builder()
                .offset(0)
                .size(std::mem::size_of::<u32>())
                .constant_id(0)
                .build()];

            let draw_gen_workgroup_size = ctx.0.properties.limits.max_compute_work_group_size[0];
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

        unsafe {
            ctx.0.device.destroy_shader_module(module, None);
        }

        let module = unsafe {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: DRAW_GEN_NO_HIGHZ_CODE.as_ptr() as *const u32,
                code_size: DRAW_GEN_NO_HIGHZ_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create object culling shader module")
        };

        let draw_gen_no_highz_pipeline = unsafe {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [vk::SpecializationMapEntry::builder()
                .offset(0)
                .size(std::mem::size_of::<u32>())
                .constant_id(0)
                .build()];

            let draw_gen_workgroup_size = ctx.0.properties.limits.max_compute_work_group_size[0];
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

        unsafe {
            ctx.0.device.destroy_shader_module(module, None);
        }

        // Lighting cluster generation
        let cluster_gen_pipeline_layout = unsafe {
            let layouts = [camera_pool.layout()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create cluster gen pipeline layout")
        };

        let module = unsafe {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: CLUSTER_GEN_CODE.as_ptr() as *const u32,
                code_size: CLUSTER_GEN_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create cluster generation shader module")
        };

        let cluster_gen_pipeline = unsafe {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let map_entries = [
                vk::SpecializationMapEntry::builder()
                    .offset(0)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(0)
                    .build(),
                vk::SpecializationMapEntry::builder()
                    .offset(std::mem::size_of::<u32>() as u32)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(1)
                    .build(),
                vk::SpecializationMapEntry::builder()
                    .offset(2 * std::mem::size_of::<u32>() as u32)
                    .size(std::mem::size_of::<u32>())
                    .constant_id(2)
                    .build(),
            ];

            let table_dims = [1u32, 1, 1];

            let as_bytes = bytemuck::bytes_of(&table_dims);

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
                .layout(cluster_gen_pipeline_layout)
                .build()];

            ctx.0
                .device
                .create_compute_pipelines(vk::PipelineCache::null(), &create_info, None)
                .expect("Unable to cluster generation pipeline")[0]
        };

        unsafe {
            ctx.0.device.destroy_shader_module(module, None);
        }

        let shadow_sampler = unsafe {
            let create_info = vk::SamplerCreateInfo::builder()
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .min_lod(0.0)
                .max_lod(0.0)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                .build();

            ctx.0
                .device
                .create_sampler(&create_info, None)
                .expect("unable to create shadow sampler")
        };

        let poisson_disk_sampler = unsafe {
            let create_info = vk::SamplerCreateInfo::builder()
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .min_lod(0.0)
                .max_lod(0.0)
                .anisotropy_enable(false)
                .build();

            ctx.0
                .device
                .create_sampler(&create_info, None)
                .expect("unable to create poisson disk sampler")
        };

        let brdf_lut_sampler = unsafe {
            let create_info = vk::SamplerCreateInfo::builder()
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0)
                .max_lod(0.0)
                .anisotropy_enable(false)
                .build();

            ctx.0
                .device
                .create_sampler(&create_info, None)
                .expect("unable to create brdf lut sampler")
        };

        let (poisson_disk, poisson_disk_view) = unsafe { create_disk(ctx) };

        let (brdf_lut, brdf_lut_view) = unsafe { create_brdf_lut(ctx) };

        let passes = MeshPasses {
            ctx: ctx.clone(),
            passes: Vec::default(),
            stages: Default::default(),
            draw_sky: false,
            active_stage: MeshPassStage::CameraSetup,
            active_pass: 0,
            total_objects: 0,
            dynamic_geo_query: None,
            point_lights_query: None,
            point_light_count: 0,
            object_buffers_expanded: false,
            depth_pyramid_gen: unsafe { DepthPyramidGenerator::new(ctx) },
            global_pool,
            draw_gen_pool,
            shadow_sampler,
            poisson_disk_sampler,
            light_clustering_pool,
            camera_pool,
            object_info,
            point_lights,
            draw_gen_pipeline_layout,
            draw_gen_pipeline,
            draw_gen_no_highz_pipeline,
            point_light_gen_pipeline_layout,
            point_light_gen_pipeline,
            cluster_gen_pipeline_layout,
            cluster_gen_pipeline,
            skybox_pipeline_layout: vk::PipelineLayout::null(),
            skybox_pipeline: vk::Pipeline::null(),
            poisson_disk,
            poisson_disk_view,
            brdf_lut,
            brdf_lut_view,
            brdf_lut_sampler,
        };

        MeshPassesBuilder {
            builder,
            ctx: ctx.clone(),
            lighting,
            passes,
        }
    }

    pub fn add_pass(&mut self, create_info: MeshPassCreateInfo) -> MeshPassId {
        // Add to stages
        let idx = self.passes.passes.len();
        self.passes.stages[MeshPassStage::CameraSetup.as_idx()].push(idx);

        if create_info.highz_culling {
            self.passes.stages[MeshPassStage::HighZRender.as_idx()].push(idx);
            self.passes.stages[MeshPassStage::HighZGenerate.as_idx()].push(idx);
        }

        self.passes.stages[MeshPassStage::GenerateDrawCalls.as_idx()].push(idx);

        if create_info.color_image.is_some() {
            self.passes.stages[MeshPassStage::ClusterLights.as_idx()].push(idx);
        }

        self.passes.stages[MeshPassStage::DepthPrepass.as_idx()].push(idx);

        if create_info.color_image.is_some() {
            self.passes.stages[MeshPassStage::OpaquePass.as_idx()].push(idx);
        }

        // Create pass
        self.passes.passes.push(unsafe {
            MeshPass::new(
                &self.ctx,
                self.lighting,
                (self.passes.brdf_lut_view, self.passes.brdf_lut_sampler),
                self.builder,
                create_info,
                &mut self.passes.depth_pyramid_gen,
                &mut self.passes.camera_pool,
                &mut self.passes.draw_gen_pool,
                &mut self.passes.global_pool,
                &mut self.passes.light_clustering_pool,
            )
        });

        MeshPassId(idx)
    }

    pub fn build(mut self) -> MeshPasses {
        // Add passes to the render graph

        // Stage 1: Camera setup
        for _ in &self.passes.stages[MeshPassStage::CameraSetup.as_idx()] {
            self.builder.add_pass(PassDescriptor::ComputePass {
                toggleable: false,
                images: Vec::default(),
                buffers: Vec::default(),
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::camera_setup(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 2.a: Expand object buffers
        self.builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: MeshPasses::expand_object_buffers,
        });

        // Stage 2.b: Highz render
        for pass in &self.passes.stages[MeshPassStage::HighZRender.as_idx()] {
            let pass = &mut self.passes.passes[*pass];
            let highz_culling = pass.highz_culling.as_mut().unwrap();

            highz_culling.pass_id = self.builder.add_pass(PassDescriptor::RenderPass {
                toggleable: false,
                color_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                    image: highz_culling.image,
                    ops: Operations {
                        load: LoadOp::Clear((1.0, 0)),
                        store: true,
                    },
                }),
                images: Vec::default(),
                buffers: vec![
                    BufferAccessDescriptor {
                        buffer: self.passes.object_info,
                        access: AccessType::Read,
                    },
                    BufferAccessDescriptor {
                        buffer: pass.output_ids,
                        access: AccessType::Read,
                    },
                ],
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::highz_render(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 3: Highz generate
        for _ in &self.passes.stages[MeshPassStage::HighZGenerate.as_idx()] {
            self.builder.add_pass(PassDescriptor::CPUPass {
                toggleable: false,
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::highz_generate(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 4.a: Prepare draw calls
        self.builder.add_pass(PassDescriptor::CPUPass {
            toggleable: false,
            code: MeshPasses::prepare_draw_calls,
        });

        // Stage 4.b: Generate draw calls
        for pass in &self.passes.stages[MeshPassStage::GenerateDrawCalls.as_idx()] {
            let pass = &self.passes.passes[*pass];

            self.builder.add_pass(PassDescriptor::ComputePass {
                toggleable: false,
                images: Vec::default(),
                buffers: vec![
                    BufferAccessDescriptor {
                        buffer: self.passes.object_info,
                        access: AccessType::Read,
                    },
                    BufferAccessDescriptor {
                        buffer: pass.input_ids,
                        access: AccessType::Read,
                    },
                    BufferAccessDescriptor {
                        buffer: pass.output_ids,
                        access: AccessType::ReadWrite,
                    },
                    BufferAccessDescriptor {
                        buffer: pass.draw_calls,
                        access: AccessType::ReadWrite,
                    },
                ],
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::generate_draw_calls(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 5: Light clustering
        for pass in &self.passes.stages[MeshPassStage::ClusterLights.as_idx()] {
            let pass = &self.passes.passes[*pass];

            self.builder.add_pass(PassDescriptor::ComputePass {
                toggleable: false,
                images: Vec::default(),
                buffers: vec![
                    BufferAccessDescriptor {
                        buffer: self.passes.point_lights,
                        access: AccessType::Read,
                    },
                    BufferAccessDescriptor {
                        buffer: pass.color_rendering.as_ref().unwrap().point_lights_table,
                        access: AccessType::ReadWrite,
                    },
                ],
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::cluster_lights(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 6: Depth prepass
        for pass in &self.passes.stages[MeshPassStage::DepthPrepass.as_idx()] {
            let pass = &mut self.passes.passes[*pass];

            let mut buffers = Vec::with_capacity(4);
            buffers.push(BufferAccessDescriptor {
                buffer: self.passes.object_info,
                access: AccessType::Read,
            });

            buffers.push(BufferAccessDescriptor {
                buffer: pass.output_ids,
                access: AccessType::Read,
            });

            buffers.push(BufferAccessDescriptor {
                buffer: self.passes.point_lights,
                access: AccessType::Read,
            });

            if let Some(color_rendering) = &pass.color_rendering {
                buffers.push(BufferAccessDescriptor {
                    buffer: color_rendering.point_lights_table,
                    access: AccessType::Read,
                });
            }

            pass.depth_prepass_id = self.builder.add_pass(PassDescriptor::RenderPass {
                toggleable: false,
                color_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                    image: pass.depth_image.image,
                    ops: Operations {
                        load: pass.depth_image.ops.load,
                        store: true,
                    },
                }),
                buffers,
                images: Vec::default(),
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::depth_prepass(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        // Stage 7: Opaque pass
        for pass in &self.passes.stages[MeshPassStage::OpaquePass.as_idx()] {
            let pass = &mut self.passes.passes[*pass];
            let color_rendering = pass.color_rendering.as_mut().unwrap();

            let mut buffers = Vec::with_capacity(4);
            buffers.push(BufferAccessDescriptor {
                buffer: self.passes.object_info,
                access: AccessType::Read,
            });

            buffers.push(BufferAccessDescriptor {
                buffer: pass.output_ids,
                access: AccessType::Read,
            });

            buffers.push(BufferAccessDescriptor {
                buffer: self.passes.point_lights,
                access: AccessType::Read,
            });

            let shadow_images = match &pass.shadow_images {
                Some(images) => {
                    let mut descriptors = Vec::with_capacity(images.len());
                    for image in images {
                        descriptors.push(ImageAccessDecriptor {
                            image: *image,
                            access: AccessType::Read,
                        })
                    }
                    descriptors
                }
                None => Vec::default(),
            };

            color_rendering.pass_id = self.builder.add_pass(PassDescriptor::RenderPass {
                toggleable: false,
                color_attachments: vec![color_rendering.color_image],
                depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                    image: pass.depth_image.image,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: pass.depth_image.ops.store,
                    },
                }),
                images: shadow_images,
                buffers: vec![BufferAccessDescriptor {
                    buffer: color_rendering.point_lights_table,
                    access: AccessType::Read,
                }],
                code: |ctx, state, cb, pass, resc| {
                    MeshPass::opaque_pass(ctx, state, cb, pass, resc);
                    state.mesh_passes.next_pass();
                },
            });
        }

        self.passes
    }
}

impl MeshPasses {
    #[inline]
    pub fn get_pass(&self, id: MeshPassId) -> &MeshPass {
        &self.passes[id.0]
    }

    #[inline]
    pub fn get_pass_mut(&mut self, id: MeshPassId) -> &mut MeshPass {
        &mut self.passes[id.0]
    }

    /// Helper function to move to the next pass.
    #[inline]
    fn next_pass(&mut self) {
        loop {
            self.active_pass = self.active_pass.wrapping_add(1);
            if self.stages[self.active_stage.as_idx()].len() <= self.active_pass {
                self.active_pass = usize::MAX;
                self.active_stage = self.active_stage.next();
                continue;
            }
            break;
        }
    }

    /// Prepares the high-z culling images for the first frame by transitioning their layouts.
    pub unsafe fn transition_highz_images(
        &self,
        resources: &RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let (pool, cb) = self
            .ctx
            .0
            .create_single_use_pool(self.ctx.0.queue_family_indices.main);

        for pass in &self.passes {
            let highz_image = match &pass.highz_culling {
                Some(highz_culling) => highz_culling.image,
                None => continue,
            };

            for target in &resources.get_image(highz_image).unwrap().1 {
                let barrier = [vk::ImageMemoryBarrier::builder()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(target.image.image())
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(
                        vk::AccessFlags::TRANSFER_WRITE | vk::AccessFlags::TRANSFER_READ,
                    )
                    .build()];

                self.ctx.0.device.cmd_pipeline_barrier(
                    cb,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::BY_REGION,
                    &[],
                    &[],
                    &barrier,
                );
            }
        }

        self.ctx.0.submit_single_use_pool(self.ctx.0.main, pool, cb);
    }

    /// Gets the ID of a render pass that performs high-z rendering.
    ///
    /// All render passes that perform high-z rendering are compatible (in the Vulkan sense).
    ///
    /// Returns `None` if no pass uses high-z culling.
    pub fn get_highz_pass_id(&self) -> Option<PassId> {
        for pass in &self.passes {
            if let Some(highz_culling) = &pass.highz_culling {
                return Some(highz_culling.pass_id);
            }
        }

        None
    }

    /// Gets the ID of a render pass that performs depth prepass.
    ///
    /// All render passes that perform depth prepass are compatible (in the Vulkan sense).
    ///
    /// Returns `None` if no pass uses depth prepass.
    pub fn get_depth_prepass_id(&self) -> Option<PassId> {
        if self.passes.is_empty() {
            None
        } else {
            Some(self.passes[0].depth_prepass_id)
        }
    }

    /// Gets the ID of a render pass that performs opaque rendering.
    ///
    /// All render passes that perform opaque rendering are compatible (in the Vulkan sense).
    ///
    /// Returns `None` if no pass uses opaque rendering.
    pub fn get_opaque_pass_id(&self) -> Option<PassId> {
        for pass in &self.passes {
            if let Some(color_rendering) = &pass.color_rendering {
                return Some(color_rendering.pass_id);
            }
        }

        None
    }

    /// Gets the active mesh pass.
    fn get_active_pass(&self) -> &MeshPass {
        &self.passes[self.stages[self.active_stage.as_idx()][self.active_pass]]
    }

    /// Gets the active mesh pass mutably.
    fn get_active_pass_mut(&mut self) -> &mut MeshPass {
        &mut self.passes[self.stages[self.active_stage.as_idx()][self.active_pass]]
    }

    /// Initializes the skybox pipeline with the given renderpass.
    pub fn initialize_skybox(&mut self, render_pass: vk::RenderPass) {
        let vert_module = unsafe {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: SKYBOX_VERT_CODE.as_ptr() as *const u32,
                code_size: SKYBOX_VERT_CODE.len(),
                ..Default::default()
            };

            self.ctx
                .0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create skybox vertex shader module")
        };

        let frag_module = unsafe {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: SKYBOX_FRAG_CODE.as_ptr() as *const u32,
                code_size: SKYBOX_FRAG_CODE.len(),
                ..Default::default()
            };

            self.ctx
                .0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create skybox fragment shader module")
        };

        self.skybox_pipeline_layout = unsafe {
            let layouts = [self.global_pool.layout(), self.camera_pool.layout()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .build();

            self.ctx
                .0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create skybox pipeline layout")
        };

        self.skybox_pipeline = unsafe {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder().build();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false)
                .build();

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::CLOCKWISE)
                .depth_bias_enable(false)
                .depth_bias_constant_factor(0.0)
                .depth_bias_clamp(0.0)
                .depth_bias_slope_factor(0.0)
                .build();

            let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
                .sample_shading_enable(false)
                .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                .min_sample_shading(1.0)
                .alpha_to_coverage_enable(false)
                .alpha_to_one_enable(false)
                .build();

            let stencil_state = vk::StencilOpState::builder()
                .fail_op(vk::StencilOp::KEEP)
                .pass_op(vk::StencilOp::KEEP)
                .depth_fail_op(vk::StencilOp::KEEP)
                .compare_op(vk::CompareOp::ALWAYS)
                .build();

            // NOTE: For the viewport and scissor the width and height doesn't really matter
            // because the dynamic stage can change them.
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                min_depth: 0.0,
                max_depth: 1.0,
            }];

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: 1,
                    height: 1,
                },
            }];

            let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                .viewports(&viewports)
                .scissors(&scissors)
                .build();

            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

            let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
                .dynamic_states(&dynamic_states)
                .build();

            let shader_stages = [
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert_module)
                    .name(&entry_name)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag_module)
                    .name(&entry_name)
                    .build(),
            ];

            let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::builder()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ZERO)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .build()];

            let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
                .logic_op_enable(false)
                .logic_op(vk::LogicOp::COPY)
                .attachments(&color_blend_attachment)
                .blend_constants([0.0, 0.0, 0.0, 0.0])
                .build();

            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(false)
                .depth_write_enable(false)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::EQUAL)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .build();

            let pipeline_info = [vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(self.skybox_pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)
                .build()];

            self.ctx
                .0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create skybox pipeline")[0]
        };

        unsafe {
            self.ctx.0.device.destroy_shader_module(vert_module, None);
            self.ctx.0.device.destroy_shader_module(frag_module, None);
        }
    }

    /// Expands object buffers to the maximum size that could be required.
    fn expand_object_buffers(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        _commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame = ctx.frame();
        let device = state.ctx.0.device.as_ref();

        // Set total object count
        state.mesh_passes.total_objects = state.static_geo.0.len.load(Ordering::Relaxed)
            + state.mesh_passes.dynamic_geo_query.as_ref().unwrap().len();

        state.mesh_passes.object_buffers_expanded = unsafe {
            let mut expanded = resources
                .get_buffer_mut(state.mesh_passes.object_info)
                .unwrap()
                .expect_write_storage_mut(frame)
                .expand(state.mesh_passes.total_objects * std::mem::size_of::<ObjectInfo>())
                .is_some();

            for pass in &state.mesh_passes.passes {
                expanded = resources
                    .get_buffer_mut(pass.input_ids)
                    .unwrap()
                    .expect_write_storage_mut(frame)
                    .expand(state.mesh_passes.total_objects * std::mem::size_of::<InputObjectId>())
                    .is_some()
                    || expanded;

                expanded = resources
                    .get_buffer_mut(pass.output_ids)
                    .unwrap()
                    .expect_storage_mut(frame)
                    .expand(state.mesh_passes.total_objects * std::mem::size_of::<OutputObjectId>())
                    .is_some()
                    || expanded;
            }

            expanded
        };

        // Expand point light buffer if needed
        let point_lights = state.mesh_passes.point_lights_query.as_ref().unwrap();
        let point_light_count = point_lights.len();

        unsafe {
            resources
                .get_buffer_mut(state.mesh_passes.point_lights)
                .unwrap()
                .expect_write_storage_mut(frame)
                .expand(point_light_count * std::mem::size_of::<RawPointLight>());
        }

        // Update global sets with object info and output ID buffers
        unsafe {
            for pass in &mut state.mesh_passes.passes {
                pass.update_global_set(
                    device,
                    frame,
                    state.mesh_passes.shadow_sampler,
                    state.mesh_passes.poisson_disk_sampler,
                    state.mesh_passes.poisson_disk_view,
                    resources,
                    state.mesh_passes.object_info,
                    state.mesh_passes.point_lights,
                );
            }
        }
    }

    /// Prepares data for generation draw calls on the GPU.
    fn prepare_draw_calls(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        _commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let materials = state.factory.0.materials.read().expect("mutex poisoned");
        let static_objects = state.static_geo.0.len.load(Ordering::Relaxed);
        let dynamic_objects = state.mesh_passes.dynamic_geo_query.as_ref().unwrap().len();

        // Swap the draw call buffers for every pass
        for pass in &mut state.mesh_passes.passes {
            std::mem::swap(
                pass.last_draw_calls.expect_write_storage_mut(frame_idx),
                resources
                    .get_buffer_mut(pass.draw_calls)
                    .unwrap()
                    .expect_write_storage_mut(frame_idx),
            );
        }

        // Write lights into buffer
        let point_lights_buffer = resources
            .get_buffer_mut(state.mesh_passes.point_lights)
            .unwrap()
            .expect_write_storage_mut(frame_idx);

        let point_lights = state.mesh_passes.point_lights_query.take().unwrap();
        state.mesh_passes.point_light_count = point_lights.len();

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
            point_lights_buffer.flush(
                0,
                state.mesh_passes.point_light_count * std::mem::size_of::<RawPointLight>(),
            );
        }

        // Static geometry needs to be rerendered if it has been marked dirty, or if object buffers
        // were expanded (and thus invalidated)
        let prepare_static_geo = state.static_geo.0.dirty[frame_idx].swap(false, Ordering::Relaxed)
            || state.mesh_passes.object_buffers_expanded;

        // Prepare object info. This is shared between all mesh passes.
        let object_info_buffer = resources
            .get_buffer(state.mesh_passes.object_info)
            .unwrap()
            .expect_write_storage(frame_idx);

        if prepare_static_geo {
            let sorted_keys = state
                .static_geo
                .0
                .sorted_keys
                .read()
                .expect("mutex poisoned");
            let mut cur_offset = 0;

            for key in sorted_keys.iter() {
                let batch = state.static_geo.0.batches.get(key).unwrap();
                let material = materials.get(batch.material.id).expect("invalid material");

                for i in 0..batch.models.len() {
                    unsafe {
                        object_info_buffer.write(
                            cur_offset,
                            ObjectInfo {
                                model: batch.models[i],
                                material_idx: material.material_slot.unwrap_or(0),
                                textures_idx: material.texture_slot.unwrap_or(0),
                                _pad: Vec2::ZERO,
                            },
                        );
                    }
                    cur_offset += 1;
                }
            }

            unsafe {
                object_info_buffer.flush(0, cur_offset * std::mem::size_of::<ObjectInfo>());
            }
        }

        let object_info_map = object_info_buffer.map();
        for (i, (renderable, model)) in state
            .mesh_passes
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
                    material_idx: material.material_slot.unwrap_or(0),
                    textures_idx: material.texture_slot.unwrap_or(0),
                    _pad: Vec2::ZERO,
                };
            }
        }

        if dynamic_objects > 0 {
            unsafe {
                object_info_buffer.flush(
                    static_objects * std::mem::size_of::<ObjectInfo>(),
                    dynamic_objects * std::mem::size_of::<ObjectInfo>(),
                );
            }
        }

        // We can prepare each pass in parallel since they are independent
        let static_geo = state.static_geo.clone();
        state.mesh_passes.passes.par_iter_mut().for_each(|pass| {
            pass.prepare_input_ids(frame_idx, static_geo.clone(), resources, prepare_static_geo);
        });

        // Now that input ids are prepared, we know how many draw calls we're going to need, so
        // we can expand each mesh passes draw call buffers
        for mesh_pass in &mut state.mesh_passes.passes {
            let total_draws = mesh_pass.dynamic_draw_calls + mesh_pass.static_draw_calls;

            let draw_call_buffer = resources
                .get_buffer_mut(mesh_pass.draw_calls)
                .unwrap()
                .expect_write_storage_mut(frame_idx);

            unsafe {
                draw_call_buffer.expand(total_draws * std::mem::size_of::<DrawCall>());
            }
        }

        // Now that the buffers are expanded, we can actually fill out the draw calls in parallel
        let factory = state.factory.clone();
        state.mesh_passes.passes.par_iter_mut().for_each(|pass| {
            pass.prepare_draw_calls(frame_idx, factory.clone(), resources);
        });
    }
}

impl Drop for MeshPasses {
    fn drop(&mut self) {
        unsafe {
            let device = self.ctx.0.device.as_ref();

            for mut pass in self.passes.drain(..) {
                pass.release(&mut self.depth_pyramid_gen);
            }

            device.destroy_image_view(self.brdf_lut_view, None);
            device.destroy_image_view(self.poisson_disk_view, None);

            device.destroy_sampler(self.shadow_sampler, None);
            device.destroy_sampler(self.poisson_disk_sampler, None);
            device.destroy_sampler(self.brdf_lut_sampler, None);

            device.destroy_pipeline_layout(self.skybox_pipeline_layout, None);
            device.destroy_pipeline_layout(self.cluster_gen_pipeline_layout, None);
            device.destroy_pipeline_layout(self.draw_gen_pipeline_layout, None);
            device.destroy_pipeline_layout(self.point_light_gen_pipeline_layout, None);

            device.destroy_pipeline(self.skybox_pipeline, None);
            device.destroy_pipeline(self.cluster_gen_pipeline, None);
            device.destroy_pipeline(self.draw_gen_no_highz_pipeline, None);
            device.destroy_pipeline(self.draw_gen_pipeline, None);
            device.destroy_pipeline(self.point_light_gen_pipeline, None);
        }
    }
}

impl MeshPassStage {
    #[inline]
    const fn count() -> usize {
        7
    }

    #[inline]
    fn as_idx(self) -> usize {
        match self {
            MeshPassStage::CameraSetup => 0,
            MeshPassStage::HighZRender => 1,
            MeshPassStage::HighZGenerate => 2,
            MeshPassStage::GenerateDrawCalls => 3,
            MeshPassStage::ClusterLights => 4,
            MeshPassStage::DepthPrepass => 5,
            MeshPassStage::OpaquePass => 6,
        }
    }

    #[inline]
    fn next(self) -> MeshPassStage {
        match self {
            MeshPassStage::CameraSetup => MeshPassStage::HighZRender,
            MeshPassStage::HighZRender => MeshPassStage::HighZGenerate,
            MeshPassStage::HighZGenerate => MeshPassStage::GenerateDrawCalls,
            MeshPassStage::GenerateDrawCalls => MeshPassStage::ClusterLights,
            MeshPassStage::ClusterLights => MeshPassStage::DepthPrepass,
            MeshPassStage::DepthPrepass => MeshPassStage::OpaquePass,
            MeshPassStage::OpaquePass => MeshPassStage::CameraSetup,
        }
    }
}

/// Helper function to create the poisson disk.
unsafe fn create_disk(ctx: &GraphicsContext) -> (Image, vk::ImageView) {
    let poisson_disk_image = {
        let create_info = ImageCreateInfo {
            ctx: ctx.clone(),
            ty: vk::ImageType::TYPE_3D,
            width: POISSON_DISK_DIMS,
            height: POISSON_DISK_DIMS,
            depth: POISSON_DISK_DIMS,
            memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
            image_usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            mip_levels: 1,
            array_layers: 1,
            format: vk::Format::R32G32B32A32_SFLOAT,
            flags: vk::ImageCreateFlags::empty(),
        };

        Image::new(&create_info)
    };

    let poisson_disk_image_view = {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(poisson_disk_image.image())
            .view_type(vk::ImageViewType::TYPE_3D)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        ctx.0
            .device
            .create_image_view(&create_info, None)
            .expect("unable to create poisson disk image view")
    };

    let staging_buffer = Buffer::new_staging_buffer(ctx, POISSON_DISK_ANGLES);

    // Upload staging buffer to error image
    let (command_pool, commands) = ctx
        .0
        .create_single_use_pool(ctx.0.queue_family_indices.transfer);

    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .image(poisson_disk_image.image())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];

    ctx.0.device.cmd_pipeline_barrier(
        commands,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::BY_REGION,
        &[],
        &[],
        &barrier,
    );

    let regions = [vk::BufferImageCopy::builder()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width: POISSON_DISK_DIMS,
            height: POISSON_DISK_DIMS,
            depth: POISSON_DISK_DIMS,
        })
        .build()];

    ctx.0.device.cmd_copy_buffer_to_image(
        commands,
        staging_buffer.buffer(),
        poisson_disk_image.image(),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &regions,
    );

    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .src_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
        .image(poisson_disk_image.image())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];

    ctx.0.device.cmd_pipeline_barrier(
        commands,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        vk::DependencyFlags::BY_REGION,
        &[],
        &[],
        &barrier,
    );

    ctx.0
        .submit_single_use_pool(ctx.0.transfer, command_pool, commands);

    (poisson_disk_image, poisson_disk_image_view)
}

/// Helper function to create the brdf look up texture.
unsafe fn create_brdf_lut(ctx: &GraphicsContext) -> (Image, vk::ImageView) {
    let brdf_lut_image = {
        let create_info = ImageCreateInfo {
            ctx: ctx.clone(),
            ty: vk::ImageType::TYPE_2D,
            width: IBL_LUT_DIMS,
            height: IBL_LUT_DIMS,
            depth: 1,
            memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
            image_usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            mip_levels: 1,
            array_layers: 1,
            format: vk::Format::R16G16_SFLOAT,
            flags: vk::ImageCreateFlags::empty(),
        };

        Image::new(&create_info)
    };

    let brdf_lut_view = {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(brdf_lut_image.image())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R16G16_SFLOAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        ctx.0
            .device
            .create_image_view(&create_info, None)
            .expect("unable to create brdf lut image view")
    };

    let staging_buffer = Buffer::new_staging_buffer(ctx, IBL_LUT);

    // Upload staging buffer to error image
    let (command_pool, commands) = ctx
        .0
        .create_single_use_pool(ctx.0.queue_family_indices.transfer);

    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .image(brdf_lut_image.image())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];

    ctx.0.device.cmd_pipeline_barrier(
        commands,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::BY_REGION,
        &[],
        &[],
        &barrier,
    );

    let regions = [vk::BufferImageCopy::builder()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width: IBL_LUT_DIMS,
            height: IBL_LUT_DIMS,
            depth: 1,
        })
        .build()];

    ctx.0.device.cmd_copy_buffer_to_image(
        commands,
        staging_buffer.buffer(),
        brdf_lut_image.image(),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &regions,
    );

    let barrier = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .src_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
        .image(brdf_lut_image.image())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];

    ctx.0.device.cmd_pipeline_barrier(
        commands,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        vk::DependencyFlags::BY_REGION,
        &[],
        &[],
        &barrier,
    );

    ctx.0
        .submit_single_use_pool(ctx.0.transfer, command_pool, commands);

    (brdf_lut_image, brdf_lut_view)
}

const DRAW_GEN_CODE: &[u8] = include_bytes!("../../draw_gen.comp.spv");

const DRAW_GEN_NO_HIGHZ_CODE: &[u8] = include_bytes!("../../draw_gen_no_highz.comp.spv");

const POINT_LIGHT_GEN_CODE: &[u8] = include_bytes!("../../point_light_gen.comp.spv");

const CLUSTER_GEN_CODE: &[u8] = include_bytes!("../../cluster_gen.comp.spv");

const SKYBOX_FRAG_CODE: &[u8] = include_bytes!("../../skybox.frag.spv");

const SKYBOX_VERT_CODE: &[u8] = include_bytes!("../../skybox.vert.spv");

const POISSON_DISK_DIMS: u32 = 32;

const POISSON_DISK_ANGLES: &[u8] = include_bytes!("./random_disk.bin");

const IBL_LUT_DIMS: u32 = 512;

const IBL_LUT: &[u8] = include_bytes!("./ibl_brdf_lut.bin");
