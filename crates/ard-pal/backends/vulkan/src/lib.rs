use api::{
    acceleration_structure::{
        BottomLevelAccelerationStructureCreateError, BottomLevelAccelerationStructureCreateInfo,
    },
    buffer::{BufferCreateError, BufferCreateInfo, BufferViewError},
    command_buffer::{BlitDestination, BlitSource, Command},
    compute_pass::ComputePassDispatch,
    compute_pipeline::{ComputePipelineCreateError, ComputePipelineCreateInfo},
    context::{GraphicsProperties, MeshShadingProperties},
    cube_map::{CubeMapCreateError, CubeMapCreateInfo},
    descriptor_set::{
        DescriptorSetCreateError, DescriptorSetCreateInfo, DescriptorSetLayoutCreateError,
        DescriptorSetLayoutCreateInfo, DescriptorSetUpdate,
    },
    graphics_pipeline::{GraphicsPipelineCreateError, GraphicsPipelineCreateInfo},
    queue::SurfacePresentFailure,
    render_pass::{ColorAttachmentDestination, DepthStencilAttachmentDestination},
    shader::{ShaderCreateError, ShaderCreateInfo},
    surface::{
        SurfaceCapabilities, SurfaceConfiguration, SurfaceCreateError, SurfaceCreateInfo,
        SurfaceImageAcquireError, SurfacePresentSuccess, SurfaceUpdateError,
    },
    texture::{TextureCreateError, TextureCreateInfo},
    types::*,
    Backend,
};
use ash::vk::{self, DebugUtilsMessageSeverityFlagsEXT};
use blas::BottomLevelAccelerationStructure;
use buffer::Buffer;
use compute_pipeline::{ComputePipeline, DispatchIndirect};
use crossbeam_utils::sync::ShardedLock;
use cube_map::CubeMap;
use descriptor_set::{DescriptorSet, DescriptorSetLayout};
use gpu_allocator::vulkan::*;
use graphics_pipeline::GraphicsPipeline;
use job::Job;
use queue::VkQueue;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use render_pass::{DrawIndexedIndirect, FramebufferCache, RenderPassCache, VkRenderPass};
use shader::Shader;
use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    mem::ManuallyDrop,
    ops::Shr,
    ptr::NonNull,
    sync::Mutex,
};
use surface::{Surface, SurfaceImage};
use texture::Texture;
use thiserror::Error;
use util::{
    command_sort::CommandSorting,
    descriptor_pool::DescriptorPools,
    garbage_collector::{GarbageCleanupArgs, GarbageCollector, TimelineValues},
    id_gen::IdGenerator,
    pipeline_cache::PipelineCache,
    queries::Queries,
    sampler_cache::SamplerCache,
    semaphores::{SemaphoreTracker, WaitInfo},
    usage::GlobalResourceUsage,
};

use crate::util::command_sort::CommandSortingInfo;

pub mod blas;
pub mod buffer;
pub mod compute_pipeline;
pub mod cube_map;
pub mod descriptor_set;
pub mod graphics_pipeline;
pub mod job;
pub mod queue;
pub mod render_pass;
pub mod shader;
pub mod surface;
pub mod texture;
pub mod util;

pub struct VulkanBackendCreateInfo<'a, W: HasRawWindowHandle + HasRawDisplayHandle> {
    pub app_name: String,
    pub engine_name: String,
    /// A window is required to find a queue that supports presentation.
    pub window: &'a W,
    /// Enables debugging layers and extensions.
    pub debug: bool,
}

#[derive(Debug, Error)]
pub enum VulkanBackendCreateError {
    #[error("vulkan error: {0}")]
    Vulkan(vk::Result),
    #[error("ash load error: {0}")]
    AshLoadError(ash::LoadingError),
    #[error("no suitable graphics device was found")]
    NoDevice,
}

pub struct VulkanBackend {
    pub(crate) entry: ash::Entry,
    pub(crate) instance: ash::Instance,
    pub(crate) debug: Option<(ash::extensions::ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) queue_family_indices: QueueFamilyIndices,
    pub(crate) properties: vk::PhysicalDeviceProperties,
    pub(crate) accel_struct_props: vk::PhysicalDeviceAccelerationStructurePropertiesKHR,
    pub(crate) graphics_properties: GraphicsProperties,
    pub(crate) _features: vk::PhysicalDeviceFeatures,
    pub(crate) device: ash::Device,
    pub(crate) surface_loader: ash::extensions::khr::Surface,
    pub(crate) swapchain_loader: ash::extensions::khr::Swapchain,
    pub(crate) mesh_shading_loader: ash::extensions::ext::MeshShader,
    pub(crate) rt_loader: ash::extensions::khr::RayTracingPipeline,
    pub(crate) as_loader: ash::extensions::khr::AccelerationStructure,
    pub(crate) main: ShardedLock<VkQueue>,
    pub(crate) transfer: ShardedLock<VkQueue>,
    pub(crate) present: ShardedLock<VkQueue>,
    pub(crate) compute: ShardedLock<VkQueue>,
    pub(crate) allocator: ManuallyDrop<Mutex<Allocator>>,
    pub(crate) render_passes: RenderPassCache,
    pub(crate) framebuffers: FramebufferCache,
    pub(crate) garbage: GarbageCollector,
    pub(crate) queries: Mutex<Queries>,
    pub(crate) buffer_ids: IdGenerator,
    pub(crate) image_ids: IdGenerator,
    pub(crate) set_ids: IdGenerator,
    pub(crate) resource_state: ShardedLock<GlobalResourceUsage>,
    pub(crate) cmd_sort: Mutex<CommandSorting>,
    pub(crate) pools: Mutex<DescriptorPools>,
    pub(crate) pipelines: Mutex<PipelineCache>,
    pub(crate) samplers: Mutex<SamplerCache>,
}

#[derive(Default)]
pub(crate) struct QueueFamilyIndices {
    /// Must support graphics, transfer, and compute.
    pub main: u32,
    /// Must support presentation.
    pub present: u32,
    /// Must support transfer.
    pub transfer: u32,
    /// Must support compute.
    pub compute: u32,
    /// Contains all queue families which are unique (some queue families may be equivilent on
    /// certain hardware.
    pub unique: Vec<u32>,
}

struct PhysicalDeviceQuery {
    pub device: vk::PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
    pub properties: vk::PhysicalDeviceProperties2,
    pub accel_struct_props: vk::PhysicalDeviceAccelerationStructurePropertiesKHR,
    pub mesh_shading_properties: vk::PhysicalDeviceMeshShaderPropertiesEXT,
    pub features: vk::PhysicalDeviceFeatures,
}

impl Backend for VulkanBackend {
    type Buffer = Buffer;
    type Texture = Texture;
    type CubeMap = CubeMap;
    type Surface = Surface;
    type SurfaceImage = SurfaceImage;
    type Shader = Shader;
    type GraphicsPipeline = GraphicsPipeline;
    type ComputePipeline = ComputePipeline;
    type DescriptorSetLayout = DescriptorSetLayout;
    type DescriptorSet = DescriptorSet;
    type BottomLevelAccelerationStructure = BottomLevelAccelerationStructure;
    type Job = Job;
    type DrawIndexedIndirect = DrawIndexedIndirect;
    type DispatchIndirect = DispatchIndirect;

    #[inline(always)]
    unsafe fn properties(&self) -> &GraphicsProperties {
        &self.graphics_properties
    }

    #[inline(always)]
    unsafe fn create_surface<W: HasRawWindowHandle + HasRawDisplayHandle>(
        &self,
        create_info: SurfaceCreateInfo<W>,
    ) -> Result<Self::Surface, SurfaceCreateError> {
        Surface::new(
            self,
            create_info,
            &self.image_ids,
            &mut self.resource_state.write().unwrap(),
        )
    }

    #[inline(always)]
    unsafe fn destroy_surface(&self, surface: &mut Self::Surface) {
        // This shouldn't happen often, so we'll wait for all work to complete
        self.device.device_wait_idle().unwrap();
        surface.release(
            self,
            &self.image_ids,
            &mut self.resource_state.write().unwrap(),
        );
        self.surface_loader.destroy_surface(surface.surface, None);
    }

    #[inline(always)]
    unsafe fn update_surface(
        &self,
        surface: &mut Self::Surface,
        config: SurfaceConfiguration,
    ) -> Result<(u32, u32), SurfaceUpdateError> {
        self.device.device_wait_idle().unwrap();

        // Signal that the views are about to be destroyed
        for (_, _, view) in &surface.images {
            self.framebuffers.view_destroyed(&self.device, *view);
        }

        // Then update the config
        surface.update_config(
            self,
            config,
            &self.image_ids,
            &mut self.resource_state.write().unwrap(),
        )
    }

    #[inline(always)]
    unsafe fn get_surface_capabilities(&self, id: &Self::Surface) -> SurfaceCapabilities {
        let capabilities = self
            .surface_loader
            .get_physical_device_surface_capabilities(self.physical_device, id.surface)
            .unwrap();

        SurfaceCapabilities {
            min_size: (
                capabilities.min_image_extent.width,
                capabilities.min_image_extent.height,
            ),
            max_size: (
                capabilities.max_image_extent.width,
                capabilities.max_image_extent.height,
            ),
            present_modes: Vec::default(), // TODO
        }
    }

    #[inline(always)]
    unsafe fn acquire_image(
        &self,
        surface: &mut Self::Surface,
    ) -> Result<Self::SurfaceImage, SurfaceImageAcquireError> {
        surface.acquire_image(self)
    }

    #[inline(always)]
    unsafe fn present_image(
        &self,
        surface: &Self::Surface,
        image: &mut Self::SurfaceImage,
    ) -> Result<SurfacePresentSuccess, SurfacePresentFailure> {
        surface.present(
            image,
            &self.swapchain_loader,
            self.present.try_read().unwrap().queue,
        )
    }

    #[inline(always)]
    unsafe fn destroy_surface_image(&self, image: &mut Self::SurfaceImage) {
        if !image.is_signaled() {
            todo!()
        }
    }

    unsafe fn submit_commands(
        &self,
        queue: QueueType,
        debug_name: Option<&str>,
        commands: Vec<Command<'_, Self>>,
        is_async: bool,
    ) -> Job {
        puffin::profile_function!();
        self.submit_commands_inner(queue, debug_name, commands, is_async, None)
    }

    unsafe fn submit_commands_async_compute(
        &self,
        queue: QueueType,
        debug_name: Option<&str>,
        commands: Vec<Command<'_, Self>>,
        compute_commands: Vec<Command<'_, Self>>,
    ) -> (Self::Job, Self::Job) {
        puffin::profile_function!();

        // Submit to the primary queue first
        let prim_job = self.submit_commands_inner(queue, debug_name, commands, false, None);

        // Then submit the async compute job
        let comp_debug_name = debug_name.map(|name| format!("{name} (Async Compute)"));
        let comp_job = self.submit_commands_inner(
            QueueType::Compute,
            comp_debug_name.as_ref().map(|n| n.as_str()),
            compute_commands,
            false,
            Some(&prim_job),
        );

        (prim_job, comp_job)
    }

    unsafe fn wait_on(&self, job: &Self::Job, timeout: Option<std::time::Duration>) -> JobStatus {
        let mut queue = match job.ty {
            QueueType::Main => self.main.write().unwrap(),
            QueueType::Transfer => self.transfer.write().unwrap(),
            QueueType::Compute => self.compute.write().unwrap(),
            QueueType::Present => self.present.write().unwrap(),
        };

        // See if we've already synced to this value
        if queue.cpu_sync_value() >= job.target_value {
            return JobStatus::Complete;
        }

        // Otherwise we have to wait
        let semaphore = [queue.semaphore()];
        let value = [job.target_value];
        let wait = vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphore)
            .values(&value)
            .build();

        match self.device.wait_semaphores(
            &wait,
            match timeout {
                Some(timeout) => timeout.as_millis() as u64,
                None => u64::MAX,
            },
        ) {
            Ok(_) => {
                queue.set_cpu_sync_value(job.target_value);
                JobStatus::Complete
            }
            Err(_) => JobStatus::Running,
        }
    }

    unsafe fn poll_status(&self, job: &Self::Job) -> JobStatus {
        let queue = match job.ty {
            QueueType::Main => self.main.read().unwrap(),
            QueueType::Transfer => self.transfer.read().unwrap(),
            QueueType::Compute => self.compute.read().unwrap(),
            QueueType::Present => self.present.read().unwrap(),
        };
        let semaphore = queue.semaphore();
        if self.device.get_semaphore_counter_value(semaphore).unwrap() >= job.target_value {
            JobStatus::Complete
        } else {
            JobStatus::Running
        }
    }

    #[inline(always)]
    unsafe fn create_buffer(
        &self,
        create_info: BufferCreateInfo,
    ) -> Result<Self::Buffer, BufferCreateError> {
        Buffer::new(
            &self.device,
            &self.queue_family_indices,
            self.debug.as_ref().map(|(utils, _)| utils),
            self.garbage.sender(),
            &self.buffer_ids,
            &mut self.allocator.lock().unwrap(),
            &self.properties.limits,
            &self.accel_struct_props,
            create_info,
        )
    }

    #[inline(always)]
    unsafe fn create_texture(
        &self,
        create_info: TextureCreateInfo,
    ) -> Result<Self::Texture, TextureCreateError> {
        Texture::new(
            &self.device,
            &self.image_ids,
            &self.queue_family_indices,
            self.debug.as_ref().map(|(utils, _)| utils),
            self.garbage.sender(),
            &mut self.allocator.lock().unwrap(),
            create_info,
        )
    }

    #[inline(always)]
    unsafe fn create_cube_map(
        &self,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self::CubeMap, CubeMapCreateError> {
        CubeMap::new(
            &self.device,
            &self.image_ids,
            &self.queue_family_indices,
            self.debug.as_ref().map(|(utils, _)| utils),
            self.garbage.sender(),
            &mut self.allocator.lock().unwrap(),
            create_info,
        )
    }

    #[inline(always)]
    unsafe fn create_shader(
        &self,
        create_info: ShaderCreateInfo,
    ) -> Result<Self::Shader, ShaderCreateError> {
        Shader::new(
            &self.device,
            self.debug.as_ref().map(|(utils, _)| utils),
            create_info,
        )
    }

    #[inline(always)]
    unsafe fn create_graphics_pipeline(
        &self,
        create_info: GraphicsPipelineCreateInfo<Self>,
    ) -> Result<Self::GraphicsPipeline, GraphicsPipelineCreateError> {
        Ok(GraphicsPipeline::new(
            &self.device,
            self.garbage.sender(),
            create_info,
        ))
    }

    #[inline(always)]
    unsafe fn create_compute_pipeline(
        &self,
        create_info: ComputePipelineCreateInfo<Self>,
    ) -> Result<Self::ComputePipeline, ComputePipelineCreateError> {
        ComputePipeline::new(
            &self.device,
            self.debug.as_ref().map(|(utils, _)| utils),
            self.garbage.sender(),
            create_info,
        )
    }

    #[inline(always)]
    unsafe fn create_descriptor_set(
        &self,
        create_info: DescriptorSetCreateInfo<Self>,
    ) -> Result<Self::DescriptorSet, DescriptorSetCreateError> {
        DescriptorSet::new(
            &self.device,
            &mut self.pools.lock().unwrap(),
            self.garbage.sender(),
            self.debug.as_ref().map(|(utils, _)| utils),
            create_info,
            &self.set_ids,
        )
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &self,
        create_info: BottomLevelAccelerationStructureCreateInfo<Self>,
    ) -> Result<Self::BottomLevelAccelerationStructure, BottomLevelAccelerationStructureCreateError>
    {
        BottomLevelAccelerationStructure::new(self, create_info)
    }

    #[inline(always)]
    unsafe fn create_descriptor_set_layout(
        &self,
        create_info: DescriptorSetLayoutCreateInfo,
    ) -> Result<Self::DescriptorSetLayout, DescriptorSetLayoutCreateError> {
        DescriptorSetLayout::new(&self.device, &mut self.pools.lock().unwrap(), create_info)
    }

    unsafe fn destroy_buffer(&self, _buffer: &mut Self::Buffer) {
        // Handled in drop
    }

    unsafe fn destroy_texture(&self, _id: &mut Self::Texture) {
        // Handled in drop
    }

    unsafe fn destroy_cube_map(&self, _id: &mut Self::CubeMap) {
        // Handled in drop
    }

    #[inline(always)]
    unsafe fn destroy_shader(&self, shader: &mut Self::Shader) {
        self.device.destroy_shader_module(shader.module, None);
    }

    unsafe fn destroy_graphics_pipeline(&self, _pipeline: &mut Self::GraphicsPipeline) {
        // Handled in drop
    }

    unsafe fn destroy_compute_pipeline(&self, _pipeline: &mut Self::ComputePipeline) {
        // Handled in drop
    }

    unsafe fn destroy_descriptor_set(&self, _set: &mut Self::DescriptorSet) {
        // Handled in drop
    }

    unsafe fn destroy_descriptor_set_layout(&self, _layout: &mut Self::DescriptorSetLayout) {
        // Not needed
    }

    unsafe fn destroy_bottom_level_acceleration_structure(
        &self,
        _id: &mut Self::BottomLevelAccelerationStructure,
    ) {
        // Handled in drop
    }

    #[inline(always)]
    unsafe fn texture_size(&self, id: &Self::Texture) -> u64 {
        id.size
    }

    #[inline(always)]
    unsafe fn cube_map_size(&self, id: &Self::CubeMap) -> u64 {
        id.size
    }

    #[inline(always)]
    unsafe fn blas_scratch_size(&self, id: &Self::BottomLevelAccelerationStructure) -> u64 {
        id.scratch_size()
    }

    #[inline(always)]
    unsafe fn blas_compacted_size(&self, id: &Self::BottomLevelAccelerationStructure) -> u64 {
        id.compacted_size(self)
    }

    #[inline(always)]
    unsafe fn blas_build_flags(
        &self,
        id: &Self::BottomLevelAccelerationStructure,
    ) -> BuildAccelerationStructureFlags {
        id.flags
    }

    #[inline(always)]
    unsafe fn map_memory(
        &self,
        id: &Self::Buffer,
        idx: usize,
    ) -> Result<(NonNull<u8>, u64), BufferViewError> {
        id.map(self, idx)
    }

    unsafe fn unmap_memory(&self, _id: &Self::Buffer) {
        // Handled by the allocator
    }

    unsafe fn flush_range(&self, _id: &Self::Buffer, _idx: usize) {
        // Not needed because `HOST_COHERENT`

        // let range = [
        //     vk::MappedMemoryRange::builder()
        //         .memory(_id.block.memory())
        //         .offset(_id.block.offset() + _id.offset(_idx))
        //         .size(_id.aligned_size)
        //         .build()
        // ];
        // self.device.flush_mapped_memory_ranges(&range).unwrap();
    }

    unsafe fn invalidate_range(&self, _id: &Self::Buffer, _idx: usize) {
        // Not needed because `HOST_COHERENT`

        // let range = [
        //     vk::MappedMemoryRange::builder()
        //         .memory(_id.block.memory())
        //         .offset(_id.block.offset() + _id.offset(_idx))
        //         .size(_id.aligned_size)
        //         .build()
        // ];
        // self.device.invalidate_mapped_memory_ranges(&range).unwrap();
    }

    #[inline(always)]
    unsafe fn update_descriptor_sets(
        &self,
        set: &mut Self::DescriptorSet,
        layout: &Self::DescriptorSetLayout,
        updates: &[DescriptorSetUpdate<Self>],
    ) {
        set.update(self, layout, updates);
    }
}

impl VulkanBackend {
    pub fn new<W: HasRawWindowHandle + HasRawDisplayHandle>(
        create_info: VulkanBackendCreateInfo<W>,
    ) -> Result<Self, VulkanBackendCreateError> {
        let app_name = CString::new(create_info.app_name).unwrap();
        let vk_version = vk::API_VERSION_1_3;

        // Get required instance layers
        let layer_names = if create_info.debug {
            vec![
                CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap(),
                CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_synchronization2\0").unwrap(),
            ]
            .into_iter()
            .map(|r| r.as_ptr())
            .collect::<Vec<_>>()
        } else {
            Vec::default()
        };

        // Get required instance extensions
        let instance_extensions = {
            let mut extensions =
                ash_window::enumerate_required_extensions(create_info.window.raw_display_handle())?
                    .iter()
                    .map(|ext| unsafe { CStr::from_ptr(*ext) })
                    .collect::<Vec<_>>();

            if create_info.debug {
                extensions.push(ash::extensions::ext::DebugUtils::name());
            }

            extensions
                .into_iter()
                .map(|r| r.as_ptr())
                .collect::<Vec<_>>()
        };

        // Get required device extensions
        let device_extensions = {
            let mut extensions = vec![
                ash::extensions::khr::Swapchain::name(),
                ash::extensions::ext::MeshShader::name(),
                ash::extensions::khr::AccelerationStructure::name(),
                ash::extensions::khr::RayTracingPipeline::name(),
                ash::extensions::khr::DeferredHostOperations::name(),
            ];
            if create_info.debug {
                extensions.push(CStr::from_bytes_with_nul(b"VK_EXT_robustness2\0").unwrap());
            }
            extensions
                .into_iter()
                .map(|r| r.as_ptr())
                .collect::<Vec<_>>()
        };

        // Dynamically load Vulkan
        let entry = unsafe { ash::Entry::load()? };

        // Create the instance
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&app_name)
            .engine_version(0)
            .api_version(vk_version);

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&instance_extensions);

        let instance = unsafe { entry.create_instance(&instance_create_info, None)? };

        // Create debugging utilities if requested
        let debug = if create_info.debug {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));
            let debug_utils_loader = ash::extensions::ext::DebugUtils::new(&entry, &instance);
            let debug_messenger =
                unsafe { debug_utils_loader.create_debug_utils_messenger(&debug_info, None)? };
            Some((debug_utils_loader, debug_messenger))
        } else {
            None
        };

        // Create a surface to check for presentation compatibility
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                create_info.window.raw_display_handle(),
                create_info.window.raw_window_handle(),
                None,
            )?
        };
        let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);

        // Query for a physical device
        let pd_query = unsafe {
            match pick_physical_device(&instance, surface, &surface_loader, &device_extensions) {
                Some(pd) => pd,
                None => return Err(VulkanBackendCreateError::NoDevice),
            }
        };

        // Cleanup surface since it's not needed anymore
        unsafe {
            surface_loader.destroy_surface(surface, None);
        }

        // Queue requests
        let mut priorities = Vec::with_capacity(pd_query.queue_family_indices.unique.len());
        let mut queue_infos = Vec::with_capacity(pd_query.queue_family_indices.unique.len());
        let mut queue_indices = (0, 0, 0, 0);
        for q in &pd_query.queue_family_indices.unique {
            let mut cur_priorities = Vec::with_capacity(4);

            if pd_query.queue_family_indices.main == *q {
                queue_indices.0 = cur_priorities.len();
                cur_priorities.push(1.0);
            }

            if pd_query.queue_family_indices.transfer == *q {
                queue_indices.1 = cur_priorities.len();
                cur_priorities.push(0.5);
            }

            if pd_query.queue_family_indices.present == *q {
                queue_indices.2 = cur_priorities.len();
                cur_priorities.push(1.0);
            }

            if pd_query.queue_family_indices.compute == *q {
                queue_indices.3 = cur_priorities.len();
                cur_priorities.push(0.5);
            }

            queue_infos.push(
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*q)
                    .queue_priorities(&cur_priorities)
                    .build(),
            );

            priorities.push(cur_priorities);
        }

        // Request features
        let features = vk::PhysicalDeviceFeatures::builder()
            .fill_mode_non_solid(true)
            .draw_indirect_first_instance(true)
            .multi_draw_indirect(true)
            .depth_clamp(true)
            .sample_rate_shading(true)
            .sampler_anisotropy(true)
            .build();

        let mut features11 = vk::PhysicalDeviceVulkan11Features::builder()
            .multiview(true)
            .storage_buffer16_bit_access(true)
            .build();

        let mut features12 = vk::PhysicalDeviceVulkan12Features::builder()
            .timeline_semaphore(true)
            .buffer_device_address(true)
            .runtime_descriptor_array(true)
            .draw_indirect_count(true)
            .uniform_buffer_standard_layout(true)
            .host_query_reset(true)
            .build();

        let mut features13 = vk::PhysicalDeviceVulkan13Features::builder()
            .synchronization2(true)
            .maintenance4(true)
            .build();

        let mut ms_features = vk::PhysicalDeviceMeshShaderFeaturesEXT::builder()
            .mesh_shader(true)
            .task_shader(true)
            .multiview_mesh_shader(true)
            .build();

        let mut rt_features = vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::builder()
            .ray_tracing_pipeline(true)
            .build();

        let mut as_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
            .acceleration_structure(true)
            .build();

        let mut features2 = vk::PhysicalDeviceFeatures2::builder()
            .features(features)
            .push_next(&mut features11)
            .push_next(&mut features12)
            .push_next(&mut features13)
            .push_next(&mut ms_features)
            .push_next(&mut rt_features)
            .push_next(&mut as_features)
            .build();

        let create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&device_extensions)
            .push_next(&mut features2);

        let create_info = create_info.build();

        // Create the device
        let device = unsafe { instance.create_device(pd_query.device, &create_info, None)? };

        // Create swapchain loader
        let swapchain_loader = ash::extensions::khr::Swapchain::new(&instance, &device);

        // Create the memory allocator
        let allocator = ManuallyDrop::new(Mutex::new(
            Allocator::new(&AllocatorCreateDesc {
                instance: instance.clone(),
                device: device.clone(),
                physical_device: pd_query.device,
                debug_settings: gpu_allocator::AllocatorDebugSettings {
                    log_memory_information: false,
                    log_leaks_on_shutdown: true,
                    store_stack_traces: false,
                    log_allocations: false,
                    log_frees: false,
                    log_stack_traces: false,
                },
                buffer_device_address: true,
                allocation_sizes: Default::default(),
            })
            .expect("unable to create GPU memory allocator"),
        ));

        // Device loaders
        let mesh_shading_loader = ash::extensions::ext::MeshShader::new(&instance, &device);
        let rt_loader = ash::extensions::khr::RayTracingPipeline::new(&instance, &device);
        let as_loader = ash::extensions::khr::AccelerationStructure::new(&instance, &device);

        // Create queues
        let main = unsafe {
            VkQueue::new(
                &device,
                debug.as_ref().map(|(utils, _)| utils),
                device.get_device_queue(pd_query.queue_family_indices.main, queue_indices.0 as u32),
                QueueType::Main,
                pd_query.queue_family_indices.main,
            )?
        };

        let transfer = unsafe {
            VkQueue::new(
                &device,
                debug.as_ref().map(|(utils, _)| utils),
                device.get_device_queue(
                    pd_query.queue_family_indices.transfer,
                    queue_indices.1 as u32,
                ),
                QueueType::Transfer,
                pd_query.queue_family_indices.transfer,
            )?
        };

        let present = unsafe {
            VkQueue::new(
                &device,
                debug.as_ref().map(|(utils, _)| utils),
                device.get_device_queue(
                    pd_query.queue_family_indices.present,
                    queue_indices.2 as u32,
                ),
                QueueType::Present,
                pd_query.queue_family_indices.present,
            )?
        };

        let compute = unsafe {
            VkQueue::new(
                &device,
                debug.as_ref().map(|(utils, _)| utils),
                device.get_device_queue(
                    pd_query.queue_family_indices.compute,
                    queue_indices.3 as u32,
                ),
                QueueType::Compute,
                pd_query.queue_family_indices.compute,
            )?
        };

        let graphics_properties = GraphicsProperties {
            mesh_shading: MeshShadingProperties {
                preferred_mesh_work_group_invocations: pd_query
                    .mesh_shading_properties
                    .max_preferred_mesh_work_group_invocations,
                preferred_task_work_group_invocations: pd_query
                    .mesh_shading_properties
                    .max_preferred_task_work_group_invocations,
            },
        };

        let ctx = Self {
            entry,
            instance,
            debug,
            physical_device: pd_query.device,
            queue_family_indices: pd_query.queue_family_indices,
            properties: pd_query.properties.properties,
            accel_struct_props: pd_query.accel_struct_props,
            graphics_properties,
            _features: pd_query.features,
            device,
            surface_loader,
            swapchain_loader,
            mesh_shading_loader,
            rt_loader,
            as_loader,
            main: ShardedLock::new(main),
            transfer: ShardedLock::new(transfer),
            present: ShardedLock::new(present),
            compute: ShardedLock::new(compute),
            allocator,
            render_passes: RenderPassCache::default(),
            framebuffers: FramebufferCache::default(),
            garbage: GarbageCollector::new(),
            queries: Mutex::new(Queries::default()),
            resource_state: ShardedLock::new(GlobalResourceUsage::default()),
            pools: Mutex::new(DescriptorPools::default()),
            pipelines: Mutex::new(PipelineCache::default()),
            samplers: Mutex::new(SamplerCache::default()),
            cmd_sort: Mutex::new(CommandSorting::default()),
            buffer_ids: IdGenerator::default(),
            image_ids: IdGenerator::default(),
            set_ids: IdGenerator::default(),
        };

        Ok(ctx)
    }

    unsafe fn submit_commands_inner<'a>(
        &self,
        queue: QueueType,
        debug_name: Option<&str>,
        commands: Vec<Command<'_, Self>>,
        is_async: bool,
        async_with: Option<&Job>,
    ) -> Job {
        // Lock down all neccesary objects
        let mut resc_state = self.resource_state.write().unwrap();
        let mut allocator = self.allocator.lock().unwrap();
        let mut pools = self.pools.lock().unwrap();
        let mut pipelines = self.pipelines.lock().unwrap();
        let mut main = self.main.write().unwrap();
        let mut transfer = self.transfer.write().unwrap();
        let mut compute = self.compute.write().unwrap();
        let mut present = self.present.write().unwrap();
        let mut sorting = self.cmd_sort.lock().unwrap();
        let mut queries = self.queries.lock().unwrap();

        // State
        let next_target_value = match queue {
            QueueType::Main => &main,
            QueueType::Transfer => &transfer,
            QueueType::Compute => &compute,
            QueueType::Present => &present,
        }
        .target_timeline_value()
            + 1;

        let mut semaphore_tracker = SemaphoreTracker::default();

        // Acquire a command buffer from the queue
        let cb = match queue {
            QueueType::Main => &mut main,
            QueueType::Transfer => &mut transfer,
            QueueType::Compute => &mut compute,
            QueueType::Present => &mut present,
        }
        .allocate_command_buffer(&self.device, self.debug.as_ref().map(|(utils, _)| utils));
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        self.device.begin_command_buffer(cb, &begin_info).unwrap();

        // Insert debug name
        if let Some(name) = debug_name {
            if let Some((debug, _)) = &self.debug {
                let name = CString::new(name).unwrap();
                let label = vk::DebugUtilsLabelEXT::builder().label_name(&name).build();
                debug.cmd_begin_debug_utils_label(cb, &label);
            }
        }

        // Generate a DAG for the submitted commands
        let mut sort_info = CommandSortingInfo {
            global: &mut resc_state,
            semaphores: &mut semaphore_tracker,
            commands: &commands,
            queue_families: &self.queue_family_indices,
            queue,
            timeline_value: next_target_value,
            wait_queues: [None; 4],
            is_async,
        };
        sorting.create_dag(&mut sort_info);

        // Execute all commands
        sorting.execute_commands(
            &self.device,
            cb,
            &commands,
            |cb, device, idx, commands| unsafe {
                VulkanBackend::execute_command(
                    cb,
                    device,
                    &self.mesh_shading_loader,
                    &self.as_loader,
                    &queries,
                    idx,
                    commands,
                    &self.render_passes,
                    &self.framebuffers,
                    &mut pipelines,
                    self.debug.as_ref(),
                );
            },
        );

        // Grab detected semaphores
        for (i, timeline_value) in sort_info.wait_queues.iter().enumerate() {
            let timeline_value = match *timeline_value {
                Some(value) => value,
                None => continue,
            };

            let detected_qt = match i {
                0 => QueueType::Main,
                1 => QueueType::Transfer,
                2 => QueueType::Compute,
                3 => QueueType::Present,
                _ => unreachable!(),
            };

            if queue == detected_qt {
                continue;
            }

            let semaphore = match detected_qt {
                QueueType::Main => main.semaphore(),
                QueueType::Transfer => transfer.semaphore(),
                QueueType::Compute => compute.semaphore(),
                QueueType::Present => present.semaphore(),
            };

            // If we were asked to run async with the detected command, and the timeline values
            // match up, we run with the previous timeline value instead
            let timeline_value = match async_with {
                Some(job) => {
                    if job.ty == detected_qt && job.target_value == timeline_value {
                        timeline_value.checked_sub(1)
                    } else {
                        Some(timeline_value)
                    }
                }
                None => Some(timeline_value),
            };

            if let Some(value) = timeline_value {
                semaphore_tracker.register_wait(
                    semaphore,
                    WaitInfo {
                        value: Some(value),
                        stage: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    },
                );
            }
        }

        // Submit to the queue
        if debug_name.is_some() {
            if let Some((debug, _)) = &self.debug {
                debug.cmd_end_debug_utils_label(cb);
            }
        }

        self.device.end_command_buffer(cb).unwrap();

        match queue {
            QueueType::Main => &mut main,
            QueueType::Transfer => &mut transfer,
            QueueType::Compute => &mut compute,
            QueueType::Present => &mut present,
        }
        .submit(&self.device, cb, semaphore_tracker)
        .unwrap();

        // Perform garbage collection
        let current_values = TimelineValues {
            main: main.current_timeline_value(&self.device),
            transfer: transfer.current_timeline_value(&self.device),
            compute: compute.current_timeline_value(&self.device),
        };
        let target_values = TimelineValues {
            main: main.target_timeline_value(),
            transfer: transfer.target_timeline_value(),
            compute: compute.target_timeline_value(),
        };

        self.garbage.cleanup(GarbageCleanupArgs {
            device: &self.device,
            as_loader: &self.as_loader,
            buffer_ids: &self.buffer_ids,
            image_ids: &self.image_ids,
            set_ids: &self.set_ids,
            allocator: &mut allocator,
            pools: &mut pools,
            pipelines: &mut pipelines,
            global_usage: &mut resc_state,
            queries: &mut queries,
            current: current_values,
            target: target_values,
            override_ref_counter: false,
        });

        Job {
            ty: queue,
            target_value: next_target_value,
        }
    }

    unsafe fn execute_command<'a>(
        cb: vk::CommandBuffer,
        device: &ash::Device,
        mesh_shading: &ash::extensions::ext::MeshShader,
        as_loader: &ash::extensions::khr::AccelerationStructure,
        queries: &Queries,
        command_idx: usize,
        commands: &[Command<'a, crate::VulkanBackend>],
        render_passes: &RenderPassCache,
        framebuffers: &FramebufferCache,
        pipelines: &mut PipelineCache,
        debug: Option<&(ash::extensions::ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    ) {
        match &commands[command_idx] {
            Command::BeginRenderPass(_, _) => Self::execute_render_pass(
                cb,
                device,
                mesh_shading,
                command_idx,
                commands,
                render_passes,
                framebuffers,
                pipelines,
                debug,
            ),
            Command::BeginComputePass(_, _) => {
                Self::execute_compute_pass(cb, device, command_idx, commands, debug)
            }
            Command::CopyBufferToBuffer(copy) => {
                let src = copy.src.internal();
                let dst = copy.dst.internal();
                let region = [vk::BufferCopy::builder()
                    .dst_offset(dst.offset(copy.dst_array_element) + copy.dst_offset)
                    .src_offset(src.offset(copy.src_array_element) + copy.src_offset)
                    .size(copy.len)
                    .build()];
                device.cmd_copy_buffer(cb, src.buffer, dst.buffer, &region);
            }
            Command::CopyTextureToTexture(copy) => {
                let src = copy.src.internal();
                let dst = copy.dst.internal();
                let region = [vk::ImageCopy::builder()
                    .dst_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: dst.aspect_flags,
                        mip_level: copy.dst_mip_level as u32,
                        base_array_layer: copy.dst_array_element as u32,
                        layer_count: 1,
                    })
                    .dst_offset(vk::Offset3D {
                        x: copy.dst_offset.0 as i32,
                        y: copy.dst_offset.1 as i32,
                        z: copy.dst_offset.2 as i32,
                    })
                    .src_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: src.aspect_flags,
                        mip_level: copy.src_mip_level as u32,
                        base_array_layer: copy.src_array_element as u32,
                        layer_count: 1,
                    })
                    .src_offset(vk::Offset3D {
                        x: copy.src_offset.0 as i32,
                        y: copy.src_offset.1 as i32,
                        z: copy.src_offset.2 as i32,
                    })
                    .extent(vk::Extent3D {
                        width: copy.extent.0,
                        height: copy.extent.1,
                        depth: copy.extent.2,
                    })
                    .build()];
                device.cmd_copy_image(
                    cb,
                    src.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &region,
                );
            }
            Command::CopyBufferToTexture {
                buffer,
                texture,
                copy,
            } => {
                let src = buffer.internal();
                let dst = texture.internal();
                let copy = [vk::BufferImageCopy::builder()
                    .buffer_offset(src.offset(copy.buffer_array_element) + copy.buffer_offset)
                    .buffer_row_length(copy.buffer_row_length)
                    .buffer_image_height(copy.buffer_image_height)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: dst.aspect_flags,
                        mip_level: copy.texture_mip_level as u32,
                        base_array_layer: copy.texture_array_element as u32,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D {
                        x: copy.texture_offset.0 as i32,
                        y: copy.texture_offset.1 as i32,
                        z: copy.texture_offset.2 as i32,
                    })
                    .image_extent(vk::Extent3D {
                        width: copy.texture_extent.0,
                        height: copy.texture_extent.1,
                        depth: copy.texture_extent.2,
                    })
                    .build()];
                device.cmd_copy_buffer_to_image(
                    cb,
                    src.buffer,
                    dst.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &copy,
                );
            }
            Command::CopyTextureToBuffer {
                buffer,
                texture,
                copy,
            } => {
                let src = texture.internal();
                let dst = buffer.internal();
                let copy = [vk::BufferImageCopy::builder()
                    .buffer_offset(dst.offset(copy.buffer_array_element) + copy.buffer_offset)
                    .buffer_row_length(copy.buffer_row_length)
                    .buffer_image_height(copy.buffer_image_height)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: src.aspect_flags,
                        mip_level: copy.texture_mip_level as u32,
                        base_array_layer: copy.texture_array_element as u32,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D {
                        x: copy.texture_offset.0 as i32,
                        y: copy.texture_offset.1 as i32,
                        z: copy.texture_offset.2 as i32,
                    })
                    .image_extent(vk::Extent3D {
                        width: copy.texture_extent.0,
                        height: copy.texture_extent.1,
                        depth: copy.texture_extent.2,
                    })
                    .build()];
                device.cmd_copy_image_to_buffer(
                    cb,
                    src.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst.buffer,
                    &copy,
                );
            }
            Command::CopyBufferToCubeMap {
                buffer,
                cube_map,
                copy,
            } => {
                let size = cube_map.dim().shr(copy.cube_map_mip_level).max(1);
                let dst = cube_map.internal();
                let src = buffer.internal();
                let copy = [vk::BufferImageCopy::builder()
                    .buffer_offset(src.offset(copy.buffer_array_element) + copy.buffer_offset)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: dst.aspect_flags,
                        mip_level: copy.cube_map_mip_level as u32,
                        base_array_layer: copy.cube_map_array_element as u32 * 6,
                        layer_count: 6,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: size,
                        height: size,
                        depth: 1,
                    })
                    .build()];
                device.cmd_copy_buffer_to_image(
                    cb,
                    src.buffer,
                    dst.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &copy,
                );
            }
            Command::CopyCubeMapToBuffer {
                cube_map,
                buffer,
                copy,
            } => {
                let size = cube_map.dim().shr(copy.cube_map_mip_level).max(1);
                let src = cube_map.internal();
                let dst = buffer.internal();
                let copy = [vk::BufferImageCopy::builder()
                    .buffer_offset(dst.offset(copy.buffer_array_element) + copy.buffer_offset)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: src.aspect_flags,
                        mip_level: copy.cube_map_mip_level as u32,
                        base_array_layer: copy.cube_map_array_element as u32 * 6,
                        layer_count: 6,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: size,
                        height: size,
                        depth: 1,
                    })
                    .build()];
                device.cmd_copy_image_to_buffer(
                    cb,
                    src.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst.buffer,
                    &copy,
                );
            }
            Command::Blit {
                src,
                dst,
                blit,
                filter,
            } => {
                let (src_img, src_array_elem, src_aspect_flags) = match src {
                    BlitSource::Texture(tex) => {
                        let internal = tex.internal();
                        (
                            internal.image,
                            blit.src_array_element,
                            internal.aspect_flags,
                        )
                    }
                    BlitSource::CubeMap { cube_map, face } => {
                        let internal = cube_map.internal();
                        (
                            internal.image,
                            CubeMap::to_array_elem(blit.src_array_element, *face),
                            internal.aspect_flags,
                        )
                    }
                };

                let (dst_img, dst_array_elem, dst_aspect_flags, is_si) = match dst {
                    BlitDestination::Texture(tex) => {
                        let internal = tex.internal();
                        (
                            internal.image,
                            blit.dst_array_element,
                            internal.aspect_flags,
                            false,
                        )
                    }
                    BlitDestination::CubeMap { cube_map, face } => {
                        let internal = cube_map.internal();
                        (
                            internal.image,
                            CubeMap::to_array_elem(blit.dst_array_element, *face),
                            internal.aspect_flags,
                            false,
                        )
                    }
                    BlitDestination::SurfaceImage(si) => {
                        let internal = si.internal();
                        (internal.image(), 0, vk::ImageAspectFlags::COLOR, true)
                    }
                };

                let vk_blit = [vk::ImageBlit::builder()
                    .src_offsets([
                        vk::Offset3D {
                            x: blit.src_min.0 as i32,
                            y: blit.src_min.1 as i32,
                            z: blit.src_min.2 as i32,
                        },
                        vk::Offset3D {
                            x: blit.src_max.0 as i32,
                            y: blit.src_max.1 as i32,
                            z: blit.src_max.2 as i32,
                        },
                    ])
                    .src_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(src_aspect_flags)
                            .mip_level(blit.src_mip as u32)
                            .base_array_layer(src_array_elem as u32)
                            .layer_count(1)
                            .build(),
                    )
                    .dst_offsets([
                        vk::Offset3D {
                            x: blit.dst_min.0 as i32,
                            y: blit.dst_min.1 as i32,
                            z: blit.dst_min.2 as i32,
                        },
                        vk::Offset3D {
                            x: blit.dst_max.0 as i32,
                            y: blit.dst_max.1 as i32,
                            z: blit.dst_max.2 as i32,
                        },
                    ])
                    .dst_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(dst_aspect_flags)
                            .mip_level(blit.dst_mip as u32)
                            .base_array_layer(dst_array_elem as u32)
                            .layer_count(1)
                            .build(),
                    )
                    .build()];

                device.cmd_blit_image(
                    cb,
                    src_img,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst_img,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk_blit,
                    crate::util::to_vk_filter(*filter),
                );

                // If the dst image is a surface image, we must transition it back into a
                // presentable format and mark it as presentable
                if is_si {
                    // Grab internal surface image and signal it was "drawn to"
                    let si = match dst {
                        BlitDestination::SurfaceImage(si) => si.internal(),
                        _ => unreachable!(),
                    };
                    si.signal_draw();
                }
            }
            Command::TransferBufferOwnership { .. } => {
                // Handled in barrier
            }
            Command::TransferTextureOwnership { .. } => {
                // Handled in barrier
            }
            Command::TransferCubeMapOwnership { .. } => {
                // Handled in barrier
            }
            Command::SetTextureUsage { .. } => {
                // Handled in barrier
            }
            Command::BuildBlas {
                blas,
                scratch,
                scratch_array_element,
            } => {
                blas.internal()
                    .build(device, cb, as_loader, scratch, *scratch_array_element);
            }
            Command::WriteBlasCompactSize(blas) => {
                blas.internal().write_compact_size(cb, as_loader, queries);
            }
            Command::CompactBlas { src, dst } => {
                // Copy buffer references
                *src.internal().buffer_refs.lock().unwrap() =
                    dst.internal().buffer_refs.lock().unwrap().clone();

                // Copy acceleration structure
                let copy_info = vk::CopyAccelerationStructureInfoKHR::builder()
                    .src(src.internal().acceleration_struct)
                    .dst(dst.internal().acceleration_struct)
                    .mode(vk::CopyAccelerationStructureModeKHR::COMPACT);
                as_loader.cmd_copy_acceleration_structure(cb, &copy_info);
            }
            _ => unreachable!(),
        }
    }

    unsafe fn execute_render_pass<'a>(
        cb: vk::CommandBuffer,
        device: &ash::Device,
        mesh_shading: &ash::extensions::ext::MeshShader,
        command_idx: usize,
        commands: &[Command<'a, crate::VulkanBackend>],
        render_passes: &RenderPassCache,
        framebuffers: &FramebufferCache,
        pipelines: &mut PipelineCache,
        debug: Option<&(ash::extensions::ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    ) {
        let mut active_layout = vk::PipelineLayout::default();
        let mut active_render_pass = VkRenderPass::default();

        for command in &commands[command_idx..] {
            match command {
                Command::BeginRenderPass(descriptor, debug_name) => {
                    if let Some(name) = *debug_name {
                        if let Some((debug, _)) = debug {
                            let name = CString::new(name).unwrap();
                            let label = vk::DebugUtilsLabelEXT::builder().label_name(&name).build();
                            debug.cmd_begin_debug_utils_label(cb, &label);
                        }
                    }

                    // Get the render pass described
                    active_render_pass = render_passes.get(device, descriptor);

                    // Find the render pass
                    let mut dims = (0, 0);
                    let mut views = Vec::with_capacity(
                        descriptor.color_attachments.len()
                            + descriptor.color_resolve_attachments.len(),
                    );
                    for attachment in &descriptor.color_attachments {
                        views.push(match &attachment.dst {
                            ColorAttachmentDestination::SurfaceImage(image) => {
                                // Indicate that the surface image has been drawn to
                                image.internal().signal_draw();
                                dims = image.internal().dims();
                                image.internal().view()
                            }
                            ColorAttachmentDestination::Texture {
                                texture,
                                array_element,
                                mip_level,
                            } => {
                                dims = (
                                    texture.dims().0.shr(mip_level).max(1),
                                    texture.dims().1.shr(mip_level).max(1),
                                );
                                texture.internal().get_view(*array_element, *mip_level)
                            }
                            ColorAttachmentDestination::CubeFace {
                                cube_map,
                                array_element,
                                face,
                                mip_level,
                            } => {
                                dims = (
                                    cube_map.dim().shr(mip_level).max(1),
                                    cube_map.dim().shr(mip_level).max(1),
                                );
                                cube_map
                                    .internal()
                                    .get_face_view(*array_element, *mip_level, *face)
                            }
                            ColorAttachmentDestination::CubeMap {
                                cube_map,
                                array_element,
                                mip_level,
                            } => {
                                dims = (
                                    cube_map.dim().shr(mip_level).max(1),
                                    cube_map.dim().shr(mip_level).max(1),
                                );
                                cube_map.internal().get_view(*array_element, *mip_level)
                            }
                        });
                    }

                    for attachment in &descriptor.color_resolve_attachments {
                        views.push(match &attachment.dst {
                            ColorAttachmentDestination::SurfaceImage(image) => {
                                // Indicate that the surface image has been drawn to
                                image.internal().signal_draw();
                                dims = image.internal().dims();
                                image.internal().view()
                            }
                            ColorAttachmentDestination::Texture {
                                texture,
                                array_element,
                                mip_level,
                            } => {
                                dims = (
                                    texture.dims().0.shr(mip_level).max(1),
                                    texture.dims().1.shr(mip_level).max(1),
                                );
                                texture.internal().get_view(*array_element, *mip_level)
                            }
                            ColorAttachmentDestination::CubeFace {
                                cube_map,
                                array_element,
                                face,
                                mip_level,
                            } => {
                                dims = (
                                    cube_map.dim().shr(mip_level).max(1),
                                    cube_map.dim().shr(mip_level).max(1),
                                );
                                cube_map
                                    .internal()
                                    .get_face_view(*array_element, *mip_level, *face)
                            }
                            ColorAttachmentDestination::CubeMap {
                                cube_map,
                                array_element,
                                mip_level,
                            } => {
                                dims = (
                                    cube_map.dim().shr(mip_level).max(1),
                                    cube_map.dim().shr(mip_level).max(1),
                                );
                                cube_map.internal().get_view(*array_element, *mip_level)
                            }
                        });
                    }

                    fn deptch_stencil_attachment_get_view(
                        dst: &DepthStencilAttachmentDestination<'_, crate::VulkanBackend>,
                    ) -> (vk::ImageView, u32, u32) {
                        match dst {
                            DepthStencilAttachmentDestination::Texture {
                                texture,
                                array_element,
                                mip_level,
                            } => {
                                let (width, height, _) = texture.dims();
                                let view = texture.internal().get_view(*array_element, *mip_level);
                                (view, width, height)
                            }
                            DepthStencilAttachmentDestination::CubeFace {
                                cube_map,
                                array_element,
                                face,
                                mip_level,
                            } => {
                                let dim = cube_map.dim();
                                let view = cube_map.internal().get_face_view(
                                    *array_element,
                                    *mip_level,
                                    *face,
                                );
                                (view, dim, dim)
                            }
                            DepthStencilAttachmentDestination::CubeMap {
                                cube_map,
                                array_element,
                                mip_level,
                            } => {
                                let dim = cube_map.dim();
                                let view = cube_map.internal().get_view(*array_element, *mip_level);
                                (view, dim, dim)
                            }
                        }
                    }

                    if let Some(attachment) = &descriptor.depth_stencil_attachment {
                        let (view, width, height) =
                            deptch_stencil_attachment_get_view(&attachment.dst);
                        dims = (width, height);
                        views.push(view);
                    }

                    if let Some(attachment) = &descriptor.depth_stencil_resolve_attachment {
                        let (view, width, height) =
                            deptch_stencil_attachment_get_view(&attachment.dst);
                        dims = (width, height);
                        views.push(view);
                    }

                    // Find the framebuffer
                    let framebuffer = framebuffers.get(
                        device,
                        active_render_pass.pass,
                        views,
                        vk::Extent2D {
                            width: dims.0,
                            height: dims.1,
                        },
                    );

                    // Find clear values
                    let mut clear_values = Vec::with_capacity(descriptor.color_attachments.len());
                    for attachment in &descriptor.color_attachments {
                        if let LoadOp::Clear(clear_color) = &attachment.load_op {
                            let color = match clear_color {
                                ClearColor::RgbaF32(r, g, b, a) => vk::ClearColorValue {
                                    float32: [*r, *g, *b, *a],
                                },
                                ClearColor::RU32(r) => vk::ClearColorValue {
                                    uint32: [*r, 0, 0, 0],
                                },
                                ClearColor::D32S32(_, _) => {
                                    panic!("invalid color clear color type")
                                }
                            };
                            clear_values.push(vk::ClearValue { color });
                        } else {
                            clear_values.push(vk::ClearValue::default());
                        }
                    }

                    for attachment in &descriptor.color_resolve_attachments {
                        if let LoadOp::Clear(clear_color) = &attachment.load_op {
                            let color = match clear_color {
                                ClearColor::RgbaF32(r, g, b, a) => vk::ClearColorValue {
                                    float32: [*r, *g, *b, *a],
                                },
                                ClearColor::RU32(r) => vk::ClearColorValue {
                                    uint32: [*r, 0, 0, 0],
                                },
                                ClearColor::D32S32(_, _) => {
                                    panic!("invalid color clear color type")
                                }
                            };
                            clear_values.push(vk::ClearValue { color });
                        } else {
                            clear_values.push(vk::ClearValue::default());
                        }
                    }

                    if let Some(attachment) = &descriptor.depth_stencil_attachment {
                        if let LoadOp::Clear(clear_color) = &attachment.load_op {
                            let depth_stencil = match clear_color {
                                ClearColor::D32S32(d, s) => vk::ClearDepthStencilValue {
                                    depth: *d,
                                    stencil: *s,
                                },
                                _ => panic!("invalid depth clear color"),
                            };
                            clear_values.push(vk::ClearValue { depth_stencil })
                        } else {
                            clear_values.push(vk::ClearValue::default());
                        }
                    }

                    if let Some(attachment) = &descriptor.depth_stencil_resolve_attachment {
                        if let LoadOp::Clear(clear_color) = &attachment.load_op {
                            let depth_stencil = match clear_color {
                                ClearColor::D32S32(d, s) => vk::ClearDepthStencilValue {
                                    depth: *d,
                                    stencil: *s,
                                },
                                _ => panic!("invalid depth clear color"),
                            };
                            clear_values.push(vk::ClearValue { depth_stencil })
                        } else {
                            clear_values.push(vk::ClearValue::default());
                        }
                    }

                    // Initial viewport configuration
                    // NOTE: Viewport is flipped to account for Vulkan coordinate system
                    let viewport = [vk::Viewport {
                        width: dims.0 as f32,
                        height: -(dims.1 as f32),
                        x: 0.0,
                        y: dims.1 as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }];

                    let scissor = [vk::Rect2D {
                        extent: vk::Extent2D {
                            width: dims.0,
                            height: dims.1,
                        },
                        offset: vk::Offset2D { x: 0, y: 0 },
                    }];

                    device.cmd_set_viewport(cb, 0, &viewport);
                    device.cmd_set_scissor(cb, 0, &scissor);

                    // Begin the render pass
                    let begin_info = vk::RenderPassBeginInfo::builder()
                        .render_pass(active_render_pass.pass)
                        .clear_values(&clear_values)
                        .framebuffer(framebuffer)
                        .render_area(vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: vk::Extent2D {
                                width: dims.0,
                                height: dims.1,
                            },
                        })
                        .build();

                    let subpass_info = vk::SubpassBeginInfo::builder()
                        .contents(vk::SubpassContents::INLINE)
                        .build();

                    device.cmd_begin_render_pass2(cb, &begin_info, &subpass_info);
                }
                Command::EndRenderPass(debug_name) => {
                    device.cmd_end_render_pass(cb);
                    if debug_name.is_some() {
                        if let Some((debug, _)) = debug {
                            debug.cmd_end_debug_utils_label(cb);
                        }
                    }
                    break;
                }
                Command::BindGraphicsPipeline(pipeline) => {
                    active_layout = pipeline.internal().layout();
                    let pipeline = pipeline.internal().get(
                        device,
                        pipelines,
                        debug.as_ref().map(|(utils, _)| utils),
                        active_render_pass,
                    );
                    device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline);
                }
                Command::PushConstants { stage, data } => device.cmd_push_constants(
                    cb,
                    active_layout,
                    crate::util::to_vk_shader_stage(*stage),
                    0,
                    data,
                ),
                Command::BindDescriptorSets { sets, first, .. } => {
                    let mut vk_sets = Vec::with_capacity(sets.len());
                    for set in sets {
                        vk_sets.push(set.internal().set);
                    }

                    device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        active_layout,
                        *first as u32,
                        &vk_sets,
                        &[],
                    );
                }
                Command::BindDescriptorSetsUnchecked { sets, first, .. } => {
                    let mut vk_sets = Vec::with_capacity(sets.len());
                    for set in sets {
                        vk_sets.push(set.internal().set);
                    }

                    device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        active_layout,
                        *first as u32,
                        &vk_sets,
                        &[],
                    );
                }
                Command::BindVertexBuffers { first, binds } => {
                    let mut buffers = Vec::with_capacity(binds.len());
                    let mut offsets = Vec::with_capacity(binds.len());
                    for bind in binds {
                        let buffer = bind.buffer.internal();
                        buffers.push(buffer.buffer);
                        offsets.push(buffer.offset(bind.array_element) + bind.offset);
                    }
                    device.cmd_bind_vertex_buffers(cb, *first as u32, &buffers, &offsets);
                }
                Command::BindIndexBuffer {
                    buffer,
                    array_element,
                    offset,
                    ty,
                } => {
                    let buffer = buffer.internal();
                    device.cmd_bind_index_buffer(
                        cb,
                        buffer.buffer,
                        buffer.offset(*array_element) + offset,
                        crate::util::to_vk_index_type(*ty),
                    );
                }
                Command::Scissor {
                    attachment,
                    scissor,
                } => {
                    device.cmd_set_scissor(
                        cb,
                        *attachment as u32,
                        &[vk::Rect2D {
                            offset: vk::Offset2D {
                                x: scissor.x,
                                y: scissor.y,
                            },
                            extent: vk::Extent2D {
                                width: scissor.width,
                                height: scissor.height,
                            },
                        }],
                    );
                }
                Command::Draw {
                    vertex_count,
                    instance_count,
                    first_vertex,
                    first_instance,
                } => {
                    device.cmd_draw(
                        cb,
                        *vertex_count as u32,
                        *instance_count as u32,
                        *first_vertex as u32,
                        *first_instance as u32,
                    );
                }
                Command::DrawIndexed {
                    index_count,
                    instance_count,
                    first_index,
                    vertex_offset,
                    first_instance,
                } => {
                    device.cmd_draw_indexed(
                        cb,
                        *index_count as u32,
                        *instance_count as u32,
                        *first_index as u32,
                        *vertex_offset as i32,
                        *first_instance as u32,
                    );
                }
                Command::DrawIndexedIndirect {
                    buffer,
                    array_element,
                    offset,
                    draw_count,
                    stride,
                } => {
                    device.cmd_draw_indexed_indirect(
                        cb,
                        buffer.internal().buffer,
                        buffer.internal().offset(*array_element) + *offset,
                        *draw_count as u32,
                        *stride as u32,
                    );
                }
                Command::DrawIndexedIndirectCount {
                    draw_buffer,
                    draw_array_element,
                    draw_offset,
                    draw_stride,
                    count_buffer,
                    count_array_element,
                    count_offset,
                    max_draw_count,
                } => {
                    device.cmd_draw_indexed_indirect_count(
                        cb,
                        draw_buffer.internal().buffer,
                        draw_buffer.internal().offset(*draw_array_element) + *draw_offset,
                        count_buffer.internal().buffer,
                        count_buffer.internal().offset(*count_array_element) + *count_offset,
                        *max_draw_count as u32,
                        *draw_stride as u32,
                    );
                }
                Command::DrawMeshTasks(x, y, z) => {
                    mesh_shading.cmd_draw_mesh_tasks(cb, *x, *y, *z);
                }
                _ => unreachable!(),
            }
        }
    }

    unsafe fn execute_compute_pass<'a>(
        cb: vk::CommandBuffer,
        device: &ash::Device,
        command_idx: usize,
        commands: &[Command<'a, crate::VulkanBackend>],
        debug: Option<&(ash::extensions::ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    ) {
        let mut active_layout = vk::PipelineLayout::default();

        for command in &commands[command_idx..] {
            match command {
                Command::BeginComputePass(pipeline, debug_name) => {
                    if let Some(name) = debug_name {
                        if let Some((debug, _)) = debug {
                            let name = CString::new(*name).unwrap();
                            let label = vk::DebugUtilsLabelEXT::builder().label_name(&name).build();
                            debug.cmd_begin_debug_utils_label(cb, &label);
                        }
                    }

                    active_layout = pipeline.internal().layout;
                    device.cmd_bind_pipeline(
                        cb,
                        vk::PipelineBindPoint::COMPUTE,
                        pipeline.internal().pipeline,
                    );
                }
                Command::EndComputePass(dispatch, debug_name) => {
                    match dispatch {
                        ComputePassDispatch::Inline(x, y, z) => {
                            device.cmd_dispatch(cb, *x, *y, *z);
                        }
                        ComputePassDispatch::Indirect {
                            buffer,
                            array_element,
                            offset,
                        } => {
                            device.cmd_dispatch_indirect(
                                cb,
                                buffer.internal().buffer,
                                buffer.internal().offset(*array_element) + *offset,
                            );
                        }
                    }

                    if debug_name.is_some() {
                        if let Some((debug, _)) = debug {
                            debug.cmd_end_debug_utils_label(cb);
                        }
                    }
                    break;
                }
                Command::PushConstants { stage, data } => device.cmd_push_constants(
                    cb,
                    active_layout,
                    crate::util::to_vk_shader_stage(*stage),
                    0,
                    data,
                ),
                Command::BindDescriptorSets { sets, first, .. } => {
                    let mut vk_sets = Vec::with_capacity(sets.len());
                    for set in sets {
                        vk_sets.push(set.internal().set);
                    }

                    device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::COMPUTE,
                        active_layout,
                        *first as u32,
                        &vk_sets,
                        &[],
                    );
                }
                Command::BindDescriptorSetsUnchecked { sets, first, .. } => {
                    let mut vk_sets = Vec::with_capacity(sets.len());
                    for set in sets {
                        vk_sets.push(set.internal().set);
                    }

                    device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::COMPUTE,
                        active_layout,
                        *first as u32,
                        &vk_sets,
                        &[],
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}

impl Drop for VulkanBackend {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            let main = self.main.get_mut().unwrap();
            let transfer = self.transfer.get_mut().unwrap();
            let compute = self.compute.get_mut().unwrap();

            let resc_state = self.resource_state.get_mut().unwrap();
            let mut allocator = self.allocator.lock().unwrap();
            let mut pools = self.pools.lock().unwrap();
            let mut pipelines = self.pipelines.lock().unwrap();
            let mut samplers = self.samplers.lock().unwrap();
            let mut queries = self.queries.lock().unwrap();

            loop {
                let current = TimelineValues {
                    main: main.current_timeline_value(&self.device),
                    transfer: transfer.current_timeline_value(&self.device),
                    compute: compute.current_timeline_value(&self.device),
                };

                let target = TimelineValues {
                    main: main.target_timeline_value(),
                    transfer: transfer.target_timeline_value(),
                    compute: compute.target_timeline_value(),
                };

                self.garbage.cleanup(GarbageCleanupArgs {
                    device: &self.device,
                    as_loader: &self.as_loader,
                    buffer_ids: &self.buffer_ids,
                    image_ids: &self.image_ids,
                    set_ids: &self.set_ids,
                    allocator: &mut allocator,
                    pools: &mut pools,
                    pipelines: &mut pipelines,
                    queries: &mut queries,
                    global_usage: resc_state,
                    current,
                    target,
                    override_ref_counter: true,
                });
                if self.garbage.is_empty() {
                    break;
                }
            }

            queries.release(&self.device);
            pools.release(&self.device);
            pipelines.release_all(&self.device);
            samplers.release(&self.device);
            std::mem::drop(allocator);
            std::mem::drop(ManuallyDrop::take(&mut self.allocator));
            self.framebuffers.release(&self.device);
            self.render_passes.release(&self.device);
            self.main.get_mut().unwrap().release(&self.device);
            self.transfer.get_mut().unwrap().release(&self.device);
            self.compute.get_mut().unwrap().release(&self.device);
            self.present.get_mut().unwrap().release(&self.device);
            self.device.destroy_device(None);
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

impl QueueFamilyIndices {
    // Returns `None` if we can't fill out all queue family types.
    fn find(
        instance: &ash::Instance,
        device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
        surface_loader: &ash::extensions::khr::Surface,
    ) -> Option<QueueFamilyIndices> {
        let mut properties =
            unsafe { instance.get_physical_device_queue_family_properties(device) };
        let mut main = usize::MAX;
        let mut present = usize::MAX;
        let mut transfer = usize::MAX;
        let mut compute = usize::MAX;

        // Find main queue. Probably will end up being family 0.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && family.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && family.queue_flags.contains(vk::QueueFlags::COMPUTE)
            {
                main = family_idx;
                break;
            }
        }

        if main == usize::MAX {
            return None;
        }

        properties[main].queue_count -= 1;

        // Find presentation queue. Would be nice to be different from main.
        for (family_idx, _) in properties.iter().enumerate() {
            let surface_support = unsafe {
                match surface_loader.get_physical_device_surface_support(
                    device,
                    family_idx as u32,
                    surface,
                ) {
                    Ok(support) => support,
                    Err(_) => return None,
                }
            };

            if surface_support && properties[family_idx].queue_count > 0 {
                present = family_idx;
                if family_idx != main {
                    break;
                }
            }
        }

        if present == usize::MAX {
            return None;
        }

        properties[present].queue_count -= 1;

        // Look for a dedicated transfer queue. Supported on some devices. Fallback is main.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && properties[family_idx].queue_count > 0
            {
                transfer = family_idx;
                if family_idx != main && family_idx != present {
                    break;
                }
            }
        }

        if transfer == usize::MAX {
            return None;
        }

        properties[transfer].queue_count -= 1;

        // Look for a dedicated async compute queue. Supported on some devices. Fallback is main.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::COMPUTE)
                && properties[family_idx].queue_count > 0
            {
                compute = family_idx;
                if family_idx != main && family_idx != present && family_idx != transfer {
                    break;
                }
            }
        }

        if compute == usize::MAX {
            return None;
        }

        let unique = {
            let mut qfi_set = std::collections::HashSet::<u32>::new();
            qfi_set.insert(main as u32);
            qfi_set.insert(present as u32);
            qfi_set.insert(transfer as u32);
            qfi_set.insert(compute as u32);
            qfi_set.into_iter().collect::<Vec<_>>()
        };

        Some(QueueFamilyIndices {
            main: main as u32,
            present: present as u32,
            transfer: transfer as u32,
            compute: compute as u32,
            unique,
        })
    }

    pub fn queue_types_to_indices(&self, queue_types: QueueTypes) -> Vec<u32> {
        let mut qfi_set = std::collections::HashSet::<u32>::new();
        if queue_types.contains(QueueTypes::MAIN) {
            qfi_set.insert(self.main);
        }
        if queue_types.contains(QueueTypes::TRANSFER) {
            qfi_set.insert(self.transfer);
        }
        if queue_types.contains(QueueTypes::COMPUTE) {
            qfi_set.insert(self.compute);
        }
        if queue_types.contains(QueueTypes::PRESENT) {
            qfi_set.insert(self.present);
        }
        qfi_set.into_iter().collect::<Vec<_>>()
    }

    #[inline(always)]
    pub fn to_index(&self, queue: QueueType) -> u32 {
        match queue {
            QueueType::Main => self.main,
            QueueType::Transfer => self.transfer,
            QueueType::Compute => self.compute,
            QueueType::Present => self.present,
        }
    }
}

unsafe impl Send for VulkanBackend {}
unsafe impl Sync for VulkanBackend {}

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface: vk::SurfaceKHR,
    loader: &ash::extensions::khr::Surface,
    extensions: &[*const i8],
) -> Option<PhysicalDeviceQuery> {
    let devices = match instance.enumerate_physical_devices() {
        Ok(devices) => devices,
        Err(_) => return None,
    };

    let mut device_type = vk::PhysicalDeviceType::OTHER;
    let mut query = None;
    for device in devices {
        let mut mesh_shading_properties = vk::PhysicalDeviceMeshShaderPropertiesEXT::default();
        let mut ray_tracing = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut accel_struct_props =
            vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();

        let mut properties = vk::PhysicalDeviceProperties2::builder()
            .push_next(&mut mesh_shading_properties)
            .push_next(&mut ray_tracing)
            .push_next(&mut accel_struct_props)
            .build();

        instance.get_physical_device_properties2(device, &mut properties);
        let features = instance.get_physical_device_features(device);

        // Must support requested extensions
        if check_device_extensions(instance, device, extensions).is_some() {
            continue;
        }

        // Must support surface stuff
        let formats = match loader.get_physical_device_surface_formats(device, surface) {
            Ok(formats) => formats,
            Err(_) => continue,
        };

        let present_modes = match loader.get_physical_device_surface_present_modes(device, surface)
        {
            Ok(modes) => modes,
            Err(_) => continue,
        };

        if formats.is_empty() || present_modes.is_empty() {
            continue;
        }

        // Must support all queue family indices
        let qfi = QueueFamilyIndices::find(instance, device, surface, loader);
        if qfi.is_none() {
            continue;
        }

        // Pick this device if it's better than the old one
        if device_type_rank(properties.properties.device_type) >= device_type_rank(device_type) {
            device_type = properties.properties.device_type;
            query = Some(PhysicalDeviceQuery {
                device,
                features,
                properties,
                accel_struct_props,
                mesh_shading_properties,
                queue_family_indices: qfi.unwrap(),
            });
        }
    }

    query
}

/// Check that a physical devices supports required device extensions.
///
/// Returns `None` on a success, or `Some` containing the name of the missing extension.
unsafe fn check_device_extensions(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
    extensions: &[*const i8],
) -> Option<String> {
    let found_extensions = match instance.enumerate_device_extension_properties(device) {
        Ok(extensions) => extensions,
        Err(_) => return Some(String::default()),
    };

    for extension_name in extensions {
        let mut found = false;
        for extension_property in &found_extensions {
            let s = CStr::from_ptr(extension_property.extension_name.as_ptr());

            if CStr::from_ptr(*extension_name).eq(s) {
                found = true;
                break;
            }
        }

        if !found {
            return Some(String::from(
                CStr::from_ptr(*extension_name).to_str().unwrap(),
            ));
        }
    }

    None
}

fn device_type_rank(ty: vk::PhysicalDeviceType) -> u32 {
    match ty {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::CPU => 2,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
        _ => 0,
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    match message_id_number {
        // Ignore `OutputNotConsumed` warnings
        101294395 => return vk::FALSE,
        // Ignore false positive from depth ms resolve
        -2069901625 => return vk::FALSE,
        _ => {}
    };

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    match message_severity {
        DebugUtilsMessageSeverityFlagsEXT::VERBOSE => print!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::INFO => print!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::WARNING => print!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::ERROR => print!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        _ => {}
    }

    vk::FALSE
}

impl From<vk::Result> for VulkanBackendCreateError {
    fn from(res: vk::Result) -> Self {
        VulkanBackendCreateError::Vulkan(res)
    }
}

impl From<ash::LoadingError> for VulkanBackendCreateError {
    fn from(err: ash::LoadingError) -> Self {
        VulkanBackendCreateError::AshLoadError(err)
    }
}
