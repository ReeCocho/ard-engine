use api::{context::GraphicsProperties, surface::SurfaceCapabilities, Backend};
use raw_window_handle::HasRawWindowHandle;

pub struct EmptyBackend(GraphicsProperties);

impl Backend for EmptyBackend {
    type Buffer = ();
    type Texture = ();
    type CubeMap = ();
    type Surface = ();
    type SurfaceImage = ();
    type Shader = ();
    type GraphicsPipeline = ();
    type ComputePipeline = ();
    type DescriptorSetLayout = ();
    type DescriptorSet = ();
    type Job = ();
    type BottomLevelAccelerationStructure = ();
    type TopLevelAccelerationStructure = ();
    type DrawIndexedIndirect = ();
    type DispatchIndirect = ();

    unsafe fn properties(&self) -> &GraphicsProperties {
        &self.0
    }

    unsafe fn create_surface<W: HasRawWindowHandle>(
        &self,
        _create_info: api::surface::SurfaceCreateInfo<W>,
    ) -> Result<Self::Surface, api::surface::SurfaceCreateError> {
        Ok(())
    }

    unsafe fn destroy_surface(&self, _id: &mut Self::Surface) {}

    unsafe fn get_surface_capabilities(&self, _: &Self::Surface) -> SurfaceCapabilities {
        SurfaceCapabilities {
            min_size: (0, 0),
            max_size: (0, 0),
            present_modes: Vec::default(),
        }
    }

    unsafe fn update_surface(
        &self,
        _id: &mut Self::Surface,
        _config: api::surface::SurfaceConfiguration,
    ) -> Result<(u32, u32), api::surface::SurfaceUpdateError> {
        Ok((0, 0))
    }

    unsafe fn acquire_image(
        &self,
        _id: &mut Self::Surface,
    ) -> Result<Self::SurfaceImage, api::surface::SurfaceImageAcquireError> {
        Ok(())
    }

    unsafe fn destroy_surface_image(&self, _id: &mut Self::SurfaceImage) {}

    unsafe fn submit_commands(
        &self,
        _queue: api::types::QueueType,
        _debug_name: Option<&str>,
        _commands: Vec<api::command_buffer::Command<'_, Self>>,
        _is_async: bool,
    ) -> Self::Job {
    }

    unsafe fn submit_commands_async_compute(
        &self,
        _queue: api::types::QueueType,
        _debug_name: Option<&str>,
        _commands: Vec<api::command_buffer::Command<'_, Self>>,
        _compute_commands: Vec<api::command_buffer::Command<'_, Self>>,
    ) -> (Self::Job, Self::Job) {
        ((), ())
    }

    unsafe fn present_image(
        &self,
        _surface: &Self::Surface,
        _image: &mut Self::SurfaceImage,
    ) -> Result<api::surface::SurfacePresentSuccess, api::queue::SurfacePresentFailure> {
        Ok(api::surface::SurfacePresentSuccess::Ok)
    }

    unsafe fn wait_on(
        &self,
        _job: &Self::Job,
        _timeout: Option<std::time::Duration>,
    ) -> api::types::JobStatus {
        api::types::JobStatus::Complete
    }

    unsafe fn poll_status(&self, _job: &Self::Job) -> api::types::JobStatus {
        api::types::JobStatus::Complete
    }

    unsafe fn create_buffer(
        &self,
        _create_info: api::buffer::BufferCreateInfo,
    ) -> Result<Self::Buffer, api::buffer::BufferCreateError> {
        Ok(())
    }

    unsafe fn create_texture(
        &self,
        _create_info: api::texture::TextureCreateInfo,
    ) -> Result<Self::Texture, api::texture::TextureCreateError> {
        Ok(())
    }

    unsafe fn create_cube_map(
        &self,
        _create_info: api::cube_map::CubeMapCreateInfo,
    ) -> Result<Self::CubeMap, api::cube_map::CubeMapCreateError> {
        Ok(())
    }

    unsafe fn create_shader(
        &self,
        _create_info: api::shader::ShaderCreateInfo,
    ) -> Result<Self::Shader, api::shader::ShaderCreateError> {
        Ok(())
    }

    unsafe fn create_graphics_pipeline(
        &self,
        _create_info: api::graphics_pipeline::GraphicsPipelineCreateInfo<Self>,
    ) -> Result<Self::GraphicsPipeline, api::graphics_pipeline::GraphicsPipelineCreateError> {
        Ok(())
    }

    unsafe fn create_compute_pipeline(
        &self,
        _create_info: api::compute_pipeline::ComputePipelineCreateInfo<Self>,
    ) -> Result<Self::ComputePipeline, api::compute_pipeline::ComputePipelineCreateError> {
        Ok(())
    }

    unsafe fn create_descriptor_set(
        &self,
        _create_info: api::descriptor_set::DescriptorSetCreateInfo<Self>,
    ) -> Result<Self::DescriptorSet, api::descriptor_set::DescriptorSetCreateError> {
        Ok(())
    }

    unsafe fn create_descriptor_set_layout(
        &self,
        _create_info: api::descriptor_set::DescriptorSetLayoutCreateInfo,
    ) -> Result<Self::DescriptorSetLayout, api::descriptor_set::DescriptorSetLayoutCreateError>
    {
        Ok(())
    }

    unsafe fn destroy_buffer(&self, _id: &mut Self::Buffer) {}

    unsafe fn destroy_texture(&self, _id: &mut Self::Texture) {}

    unsafe fn destroy_cube_map(&self, _id: &mut Self::CubeMap) {}

    unsafe fn destroy_shader(&self, _id: &mut Self::Shader) {}

    unsafe fn destroy_graphics_pipeline(&self, _id: &mut Self::GraphicsPipeline) {}

    unsafe fn destroy_compute_pipeline(&self, _id: &mut Self::ComputePipeline) {}

    unsafe fn destroy_descriptor_set(&self, _id: &mut Self::DescriptorSet) {}

    unsafe fn destroy_descriptor_set_layout(&self, _id: &mut Self::DescriptorSetLayout) {}

    unsafe fn map_memory(
        &self,
        _id: &Self::Buffer,
        _idx: usize,
    ) -> Result<(std::ptr::NonNull<u8>, u64), api::buffer::BufferViewError> {
        Err(api::buffer::BufferViewError::Other(String::from(
            "cannot map memory in empty backend",
        )))
    }

    unsafe fn unmap_memory(&self, _id: &Self::Buffer) {}

    unsafe fn flush_range(&self, _id: &Self::Buffer, _idx: usize) {}

    unsafe fn invalidate_range(&self, _id: &Self::Buffer, _idx: usize) {}

    unsafe fn update_descriptor_sets(
        &self,
        _id: &mut Self::DescriptorSet,
        _layout: &Self::DescriptorSetLayout,
        _updates: &[api::descriptor_set::DescriptorSetUpdate<Self>],
    ) {
    }

    unsafe fn texture_size(&self, _id: &Self::Texture) -> u64 {
        0
    }

    unsafe fn cube_map_size(&self, _id: &Self::CubeMap) -> u64 {
        0
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &self,
        _build_info: api::blas::BottomLevelAccelerationStructureCreateInfo<Self>,
    ) -> Result<
        Self::BottomLevelAccelerationStructure,
        api::blas::BottomLevelAccelerationStructureCreateError,
    > {
        todo!()
    }

    unsafe fn destroy_bottom_level_acceleration_structure(
        &self,
        _id: &mut Self::BottomLevelAccelerationStructure,
    ) {
        todo!()
    }

    unsafe fn blas_device_ref(&self, _id: &Self::BottomLevelAccelerationStructure) -> u64 {
        todo!()
    }

    unsafe fn blas_scratch_size(&self, _id: &Self::BottomLevelAccelerationStructure) -> u64 {
        todo!()
    }

    unsafe fn blas_compacted_size(&self, _id: &Self::BottomLevelAccelerationStructure) -> u64 {
        todo!()
    }

    unsafe fn buffer_device_ref(&self, _id: &Self::Buffer, _array_element: usize) -> u64 {
        todo!()
    }

    unsafe fn blas_build_flags(
        &self,
        _id: &Self::BottomLevelAccelerationStructure,
    ) -> api::types::BuildAccelerationStructureFlags {
        todo!()
    }

    unsafe fn create_top_level_acceleration_structure(
        &self,
        _create_info: api::tlas::TopLevelAccelerationStructureCreateInfo,
    ) -> Result<
        Self::TopLevelAccelerationStructure,
        api::tlas::TopLevelAccelerationStructureCreateError,
    > {
        todo!()
    }

    unsafe fn destroy_top_level_acceleration_structure(
        &self,
        _id: &mut Self::TopLevelAccelerationStructure,
    ) {
        todo!()
    }

    unsafe fn tlas_scratch_size(&self, _id: &Self::TopLevelAccelerationStructure) -> u64 {
        todo!()
    }

    unsafe fn tlas_build_flags(
        &self,
        _id: &Self::TopLevelAccelerationStructure,
    ) -> api::types::BuildAccelerationStructureFlags {
        todo!()
    }
}
