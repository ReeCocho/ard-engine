//! Pal is a graphics and compute library inspired by [Vulkan](https://www.vulkan.org/) and
//! [wgpu](https://github.com/gfx-rs/wgpu).
//!
//! To start using Pal, you must first choose a [`Backend`] and then create a
//! [`Context`](struct@context::Context).

pub mod blas;
pub mod buffer;
pub mod command_buffer;
pub mod compute_pass;
pub mod compute_pipeline;
pub mod context;
pub mod cube_map;
pub mod descriptor_set;
pub mod graphics_pipeline;
pub mod queue;
pub mod render_pass;
pub mod rt_pass;
pub mod rt_pipeline;
pub mod shader;
pub mod surface;
pub mod texture;
pub mod tlas;
pub mod types;

use std::{ptr::NonNull, time::Duration};

use blas::{
    BottomLevelAccelerationStructureCreateError, BottomLevelAccelerationStructureCreateInfo,
};
use buffer::{BufferCreateError, BufferCreateInfo, BufferViewError};
use command_buffer::Command;
use compute_pipeline::{ComputePipelineCreateError, ComputePipelineCreateInfo};
use context::GraphicsProperties;
use cube_map::{CubeMapCreateError, CubeMapCreateInfo};
use descriptor_set::{
    DescriptorSetCreateError, DescriptorSetCreateInfo, DescriptorSetLayoutCreateError,
    DescriptorSetLayoutCreateInfo, DescriptorSetUpdate,
};
use graphics_pipeline::{GraphicsPipelineCreateError, GraphicsPipelineCreateInfo};
use queue::SurfacePresentFailure;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rt_pipeline::{
    RayTracingPipelineCreateError, RayTracingPipelineCreateInfo, ShaderBindingTableData,
};
use shader::{ShaderCreateError, ShaderCreateInfo};
use surface::{
    SurfaceCapabilities, SurfaceConfiguration, SurfaceCreateError, SurfaceCreateInfo,
    SurfaceImageAcquireError, SurfacePresentSuccess, SurfaceUpdateError,
};
use texture::{TextureCreateError, TextureCreateInfo};
use tlas::{TopLevelAccelerationStructureCreateError, TopLevelAccelerationStructureCreateInfo};
use types::{BuildAccelerationStructureFlags, JobStatus, QueueType};

/// TODO:
/// - Describe [normative terminology](https://www.ietf.org/rfc/rfc2119.txt).
/// - Explain backend requirements.
/// - Explain user requirements.
#[allow(clippy::missing_safety_doc)]
pub trait Backend: Sized + 'static {
    type Buffer;
    type Texture;
    type CubeMap;
    type Surface;
    type SurfaceImage;
    type Shader;
    type GraphicsPipeline;
    type ComputePipeline;
    type RayTracingPipeline;
    type DescriptorSetLayout;
    type DescriptorSet;
    type Job;
    type BottomLevelAccelerationStructure;
    type TopLevelAccelerationStructure;
    type DrawIndexedIndirect: Copy + Clone;
    type DispatchIndirect: Copy + Clone;

    unsafe fn properties(&self) -> &GraphicsProperties;

    // Surface
    unsafe fn create_surface<W: HasWindowHandle + HasDisplayHandle>(
        &self,
        create_info: SurfaceCreateInfo<W>,
    ) -> Result<Self::Surface, SurfaceCreateError>;
    unsafe fn destroy_surface(&self, id: &mut Self::Surface);
    unsafe fn update_surface(
        &self,
        id: &mut Self::Surface,
        config: SurfaceConfiguration,
    ) -> Result<(u32, u32), SurfaceUpdateError>;
    unsafe fn get_surface_capabilities(&self, id: &Self::Surface) -> SurfaceCapabilities;
    unsafe fn acquire_image(
        &self,
        id: &mut Self::Surface,
    ) -> Result<Self::SurfaceImage, SurfaceImageAcquireError>;
    unsafe fn destroy_surface_image(&self, id: &mut Self::SurfaceImage);

    // Command submit
    unsafe fn submit_commands(
        &self,
        queue: QueueType,
        debug_name: Option<&str>,
        commands: Vec<Command<'_, Self>>,
        is_async: bool,
    ) -> Self::Job;
    unsafe fn submit_commands_async_compute(
        &self,
        queue: QueueType,
        debug_name: Option<&str>,
        commands: Vec<Command<'_, Self>>,
        compute_commands: Vec<Command<'_, Self>>,
    ) -> (Self::Job, Self::Job);
    unsafe fn present_image(
        &self,
        surface: &Self::Surface,
        image: &mut Self::SurfaceImage,
    ) -> Result<SurfacePresentSuccess, SurfacePresentFailure>;

    // Jobs
    unsafe fn wait_on(&self, job: &Self::Job, timeout: Option<Duration>) -> JobStatus;
    unsafe fn poll_status(&self, job: &Self::Job) -> JobStatus;

    // Creating resources
    unsafe fn create_buffer(
        &self,
        create_info: BufferCreateInfo,
    ) -> Result<Self::Buffer, BufferCreateError>;
    unsafe fn create_texture(
        &self,
        create_info: TextureCreateInfo,
    ) -> Result<Self::Texture, TextureCreateError>;
    unsafe fn create_cube_map(
        &self,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self::CubeMap, CubeMapCreateError>;
    unsafe fn create_shader(
        &self,
        create_info: ShaderCreateInfo,
    ) -> Result<Self::Shader, ShaderCreateError>;
    unsafe fn create_graphics_pipeline(
        &self,
        create_info: GraphicsPipelineCreateInfo<Self>,
    ) -> Result<Self::GraphicsPipeline, GraphicsPipelineCreateError>;
    unsafe fn create_compute_pipeline(
        &self,
        create_info: ComputePipelineCreateInfo<Self>,
    ) -> Result<Self::ComputePipeline, ComputePipelineCreateError>;
    unsafe fn create_ray_tracing_pipeline(
        &self,
        create_info: RayTracingPipelineCreateInfo<Self>,
    ) -> Result<Self::RayTracingPipeline, RayTracingPipelineCreateError>;
    unsafe fn create_descriptor_set(
        &self,
        create_info: DescriptorSetCreateInfo<Self>,
    ) -> Result<Self::DescriptorSet, DescriptorSetCreateError>;
    unsafe fn create_descriptor_set_layout(
        &self,
        create_info: DescriptorSetLayoutCreateInfo,
    ) -> Result<Self::DescriptorSetLayout, DescriptorSetLayoutCreateError>;
    unsafe fn create_bottom_level_acceleration_structure(
        &self,
        create_info: BottomLevelAccelerationStructureCreateInfo<Self>,
    ) -> Result<Self::BottomLevelAccelerationStructure, BottomLevelAccelerationStructureCreateError>;
    unsafe fn create_top_level_acceleration_structure(
        &self,
        create_info: TopLevelAccelerationStructureCreateInfo,
    ) -> Result<Self::TopLevelAccelerationStructure, TopLevelAccelerationStructureCreateError>;

    // Destroying resources
    unsafe fn destroy_buffer(&self, id: &mut Self::Buffer);
    unsafe fn destroy_texture(&self, id: &mut Self::Texture);
    unsafe fn destroy_cube_map(&self, id: &mut Self::CubeMap);
    unsafe fn destroy_shader(&self, id: &mut Self::Shader);
    unsafe fn destroy_graphics_pipeline(&self, id: &mut Self::GraphicsPipeline);
    unsafe fn destroy_compute_pipeline(&self, id: &mut Self::ComputePipeline);
    unsafe fn destroy_ray_tracing_pipeline(&self, id: &mut Self::RayTracingPipeline);
    unsafe fn destroy_descriptor_set(&self, id: &mut Self::DescriptorSet);
    unsafe fn destroy_descriptor_set_layout(&self, id: &mut Self::DescriptorSetLayout);
    unsafe fn destroy_bottom_level_acceleration_structure(
        &self,
        id: &mut Self::BottomLevelAccelerationStructure,
    );
    unsafe fn destroy_top_level_acceleration_structure(
        &self,
        id: &mut Self::TopLevelAccelerationStructure,
    );

    // Memory management
    unsafe fn map_memory(
        &self,
        id: &Self::Buffer,
        idx: usize,
    ) -> Result<(NonNull<u8>, u64), BufferViewError>;
    unsafe fn unmap_memory(&self, id: &Self::Buffer);
    unsafe fn flush_range(&self, id: &Self::Buffer, idx: usize);
    unsafe fn invalidate_range(&self, id: &Self::Buffer, idx: usize);

    // Getters
    unsafe fn buffer_device_ref(&self, id: &Self::Buffer, array_element: usize) -> u64;
    unsafe fn texture_size(&self, id: &Self::Texture) -> u64;
    unsafe fn cube_map_size(&self, id: &Self::CubeMap) -> u64;
    unsafe fn tlas_scratch_size(&self, id: &Self::TopLevelAccelerationStructure) -> u64;
    unsafe fn tlas_build_flags(
        &self,
        id: &Self::TopLevelAccelerationStructure,
    ) -> BuildAccelerationStructureFlags;
    unsafe fn blas_device_ref(&self, id: &Self::BottomLevelAccelerationStructure) -> u64;
    unsafe fn blas_scratch_size(&self, id: &Self::BottomLevelAccelerationStructure) -> u64;
    unsafe fn blas_compacted_size(&self, id: &Self::BottomLevelAccelerationStructure) -> u64;
    unsafe fn blas_build_flags(
        &self,
        id: &Self::BottomLevelAccelerationStructure,
    ) -> BuildAccelerationStructureFlags;
    unsafe fn shader_binding_table_data(
        &self,
        id: &Self::RayTracingPipeline,
    ) -> ShaderBindingTableData;

    // Descriptor set
    unsafe fn update_descriptor_sets(
        &self,
        id: &mut Self::DescriptorSet,
        layout: &Self::DescriptorSetLayout,
        updates: &[DescriptorSetUpdate<Self>],
    );
}
