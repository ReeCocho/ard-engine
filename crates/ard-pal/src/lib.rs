cfg_if::cfg_if! {
    if #[cfg(feature = "vulkan")] {
        pub type Backend = vulkan::VulkanBackend;
        pub mod backend {
            pub use vulkan::{VulkanBackend, VulkanBackendCreateError, VulkanBackendCreateInfo};
        }
    } else {
        pub type Backend = empty::EmptyBackend;
        pub mod backend {
            pub use empty::EmptyBackend;
        }
    }
}

pub mod prelude {
    pub use api::types::*;

    // Context
    pub type Context = api::context::Context<crate::Backend>;
    pub type GraphicsProperties = api::context::GraphicsProperties;

    // Surface
    pub type Surface = api::surface::Surface<crate::Backend>;
    pub type SurfaceImage = api::surface::SurfaceImage<crate::Backend>;
    pub use api::surface::{
        SurfaceConfiguration, SurfaceCreateError, SurfaceCreateInfo, SurfacePresentSuccess,
    };

    // Compute pass
    pub type DispatchIndirect = <crate::Backend as api::Backend>::DispatchIndirect;
    pub type ComputePassDispatch<'a> = api::compute_pass::ComputePassDispatch<'a, crate::Backend>;

    // Render pass
    pub type DrawIndexedIndirect = <crate::Backend as api::Backend>::DrawIndexedIndirect;
    pub type RenderPass<'a> = api::render_pass::RenderPass<'a, crate::Backend>;
    pub type RenderPassDescriptor<'a> = api::render_pass::RenderPassDescriptor<'a, crate::Backend>;
    pub type ColorAttachmentDestination<'a> =
        api::render_pass::ColorAttachmentDestination<'a, crate::Backend>;
    pub use api::render_pass::{
        ColorAttachment, ColorResolveAttachment, DepthStencilAttachment,
        DepthStencilAttachmentDestination, DepthStencilResolveAttachment, VertexBind,
    };

    // Command buffer
    pub use api::command_buffer::{
        BlitDestination, BlitSource, BufferCubeMapCopy, BufferTextureCopy, CopyBufferToBuffer,
        CopyTextureToTexture, TextureResolve,
    };
    pub type CommandBuffer<'a> = api::command_buffer::CommandBuffer<'a, crate::Backend>;

    // Queue
    pub type Queue = api::queue::Queue<crate::Backend>;
    pub type Job = api::queue::Job<crate::Backend>;

    // Shader
    pub type Shader = api::shader::Shader<crate::Backend>;
    pub use api::shader::{ShaderCreateError, ShaderCreateInfo};

    // Graphics pipeline
    pub type GraphicsPipeline = api::graphics_pipeline::GraphicsPipeline<crate::Backend>;
    pub type MeshShadingShader = api::graphics_pipeline::MeshShadingShader<crate::Backend>;
    pub use api::graphics_pipeline::{
        ColorBlendAttachment, ColorBlendState, DepthStencilState, GraphicsPipelineCreateError,
        GraphicsPipelineCreateInfo, RasterizationState, ShaderStages, VertexInputAttribute,
        VertexInputBinding, VertexInputState,
    };

    // Compute pipeline
    pub type ComputePipeline = api::compute_pipeline::ComputePipeline<crate::Backend>;
    pub use api::compute_pipeline::{ComputePipelineCreateError, ComputePipelineCreateInfo};
    pub type ComputePass<'a> = api::compute_pass::ComputePass<'a, crate::Backend>;

    // Buffer
    pub type Buffer = api::buffer::Buffer<crate::Backend>;
    pub type BufferReadView<'a> = api::buffer::BufferReadView<'a, crate::Backend>;
    pub type BufferWriteView<'a> = api::buffer::BufferWriteView<'a, crate::Backend>;
    pub use api::buffer::{BufferCreateError, BufferCreateInfo, BufferViewError};

    // Texture
    pub type Texture = api::texture::Texture<crate::Backend>;
    pub use api::texture::{Blit, Sampler, TextureCreateError, TextureCreateInfo};

    // Cube map
    pub type CubeMap = api::cube_map::CubeMap<crate::Backend>;
    pub use api::cube_map::{CubeMapCreateError, CubeMapCreateInfo};

    // Descriptor set & layout
    pub type DescriptorSetLayout = api::descriptor_set::DescriptorSetLayout<crate::Backend>;
    pub type DescriptorSet = api::descriptor_set::DescriptorSet<crate::Backend>;
    pub type DescriptorValue<'a> = api::descriptor_set::DescriptorValue<'a, crate::Backend>;
    pub use api::descriptor_set::{
        DescriptorBinding, DescriptorSetCreateError, DescriptorSetCreateInfo,
        DescriptorSetLayoutCreateError, DescriptorSetLayoutCreateInfo, DescriptorSetUpdate,
        DescriptorType,
    };

    // BLAS
    pub type BottomLevelAccelerationStructure =
        api::blas::BottomLevelAccelerationStructure<crate::Backend>;
    pub type AccelerationStructureGeometry<'a> =
        api::blas::AccelerationStructureGeometry<'a, crate::Backend>;
    pub type BottomLevelAccelerationStructureCreateInfo<'a> =
        api::blas::BottomLevelAccelerationStructureCreateInfo<'a, crate::Backend>;
    pub type BottomLevelAccelerationStructureData<'a> =
        api::blas::BottomLevelAccelerationStructureData<'a, crate::Backend>;

    // TLAS
    pub use api::tlas::TopLevelAccelerationStructureCreateInfo;
    pub type TopLevelAccelerationStructure =
        api::tlas::TopLevelAccelerationStructure<crate::Backend>;
}
