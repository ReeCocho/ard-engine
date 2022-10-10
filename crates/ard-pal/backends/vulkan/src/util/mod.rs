use api::{descriptor_set::DescriptorType, types::*};
use ash::vk;
use gpu_allocator::MemoryLocation;

pub mod descriptor_pool;
pub mod fast_int_hasher;
pub mod garbage_collector;
pub mod pipeline_cache;
pub mod sampler_cache;
pub mod semaphores;
pub mod tracking;

pub mod usage;

#[inline(always)]
pub(crate) fn rank_pipeline_stage(stage: vk::PipelineStageFlags) -> u32 {
    match stage {
        vk::PipelineStageFlags::TOP_OF_PIPE => 0,
        vk::PipelineStageFlags::DRAW_INDIRECT => 1,
        vk::PipelineStageFlags::VERTEX_INPUT => 2,
        vk::PipelineStageFlags::VERTEX_SHADER => 3,
        vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER => 4,
        vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER => 5,
        vk::PipelineStageFlags::GEOMETRY_SHADER => 6,
        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS => 7,
        vk::PipelineStageFlags::FRAGMENT_SHADER => 8,
        vk::PipelineStageFlags::LATE_FRAGMENT_TESTS => 9,
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT => 10,
        vk::PipelineStageFlags::TRANSFER => 11,
        vk::PipelineStageFlags::COMPUTE_SHADER => 12,
        vk::PipelineStageFlags::BOTTOM_OF_PIPE => 13,
        _ => u32::MAX,
    }
}

#[inline(always)]
pub(crate) fn to_vk_present_mode(present_mode: PresentMode) -> vk::PresentModeKHR {
    match present_mode {
        PresentMode::Immediate => vk::PresentModeKHR::IMMEDIATE,
        PresentMode::Mailbox => vk::PresentModeKHR::MAILBOX,
        PresentMode::Fifo => vk::PresentModeKHR::FIFO,
        PresentMode::FifoRelaxed => vk::PresentModeKHR::FIFO_RELAXED,
    }
}

#[inline(always)]
pub(crate) fn to_vk_format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::R8Unorm => vk::Format::R8_UNORM,
        TextureFormat::R32Sfloat => vk::Format::R32_SFLOAT,
        TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::Rgba8Srgb => vk::Format::R8G8B8A8_SRGB,
        TextureFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        TextureFormat::Bgra8Srgb => vk::Format::B8G8R8A8_SRGB,
        TextureFormat::D16Unorm => vk::Format::D16_UNORM,
        TextureFormat::D24UnormS8Uint => vk::Format::D24_UNORM_S8_UINT,
        TextureFormat::D32Sfloat => vk::Format::D32_SFLOAT,
        TextureFormat::D32SfloatS8Uint => vk::Format::D32_SFLOAT_S8_UINT,
        TextureFormat::Rgba16SFloat => vk::Format::R16G16B16A16_SFLOAT,
    }
}

#[inline(always)]
pub(crate) fn to_vk_index_type(ty: IndexType) -> vk::IndexType {
    match ty {
        IndexType::U16 => vk::IndexType::UINT16,
        IndexType::U32 => vk::IndexType::UINT32,
    }
}

#[inline(always)]
pub(crate) fn to_vk_store_op(store_op: StoreOp) -> vk::AttachmentStoreOp {
    match store_op {
        StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
        StoreOp::Store => vk::AttachmentStoreOp::STORE,
    }
}

#[inline(always)]
pub(crate) fn to_vk_load_op(load_op: LoadOp) -> vk::AttachmentLoadOp {
    match load_op {
        LoadOp::DontCare => vk::AttachmentLoadOp::DONT_CARE,
        LoadOp::Load => vk::AttachmentLoadOp::LOAD,
        LoadOp::Clear(_) => vk::AttachmentLoadOp::CLEAR,
    }
}

#[inline(always)]
pub(crate) fn to_vk_descriptor_type(ty: DescriptorType) -> vk::DescriptorType {
    match ty {
        DescriptorType::Texture => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        DescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        DescriptorType::StorageBuffer(_) => vk::DescriptorType::STORAGE_BUFFER,
        DescriptorType::StorageImage(_) => vk::DescriptorType::STORAGE_IMAGE,
        DescriptorType::CubeMap => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    }
}

#[inline(always)]
pub(crate) fn to_vk_vertex_rate(rate: VertexInputRate) -> vk::VertexInputRate {
    match rate {
        VertexInputRate::Vertex => vk::VertexInputRate::VERTEX,
        VertexInputRate::Instance => vk::VertexInputRate::INSTANCE,
    }
}

#[inline(always)]
pub(crate) fn to_vk_vertex_format(format: VertexFormat) -> vk::Format {
    match format {
        VertexFormat::XF32 => vk::Format::R32_SFLOAT,
        VertexFormat::XyF32 => vk::Format::R32G32_SFLOAT,
        VertexFormat::XyzwF32 => vk::Format::R32G32B32A32_SFLOAT,
        VertexFormat::XyzwU8 => vk::Format::R8G8B8A8_UNORM,
    }
}

#[inline(always)]
pub(crate) fn to_vk_shader_stage(ss: ShaderStage) -> vk::ShaderStageFlags {
    match ss {
        ShaderStage::AllGraphics => vk::ShaderStageFlags::ALL_GRAPHICS,
        ShaderStage::Vertex => vk::ShaderStageFlags::VERTEX,
        ShaderStage::Fragment => vk::ShaderStageFlags::FRAGMENT,
        ShaderStage::Compute => vk::ShaderStageFlags::COMPUTE,
    }
}

#[inline(always)]
pub(crate) fn to_vk_topology(top: PrimitiveTopology) -> vk::PrimitiveTopology {
    match top {
        PrimitiveTopology::PontList => vk::PrimitiveTopology::POINT_LIST,
        PrimitiveTopology::LineList => vk::PrimitiveTopology::LINE_LIST,
        PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
    }
}

#[inline(always)]
pub(crate) fn to_vk_cull_mode(cm: CullMode) -> vk::CullModeFlags {
    match cm {
        CullMode::None => vk::CullModeFlags::NONE,
        CullMode::Front => vk::CullModeFlags::FRONT,
        CullMode::Back => vk::CullModeFlags::BACK,
        CullMode::FrontAndBack => vk::CullModeFlags::FRONT_AND_BACK,
    }
}

#[inline(always)]
pub(crate) fn to_vk_front_face(ff: FrontFace) -> vk::FrontFace {
    match ff {
        FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
        FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
    }
}

#[inline(always)]
pub(crate) fn to_vk_polygon_mode(pm: PolygonMode) -> vk::PolygonMode {
    match pm {
        PolygonMode::Fill => vk::PolygonMode::FILL,
        PolygonMode::Line => vk::PolygonMode::LINE,
        PolygonMode::Point => vk::PolygonMode::POINT,
    }
}

#[inline(always)]
pub(crate) fn to_vk_compare_op(co: CompareOp) -> vk::CompareOp {
    match co {
        CompareOp::Never => vk::CompareOp::NEVER,
        CompareOp::Less => vk::CompareOp::LESS,
        CompareOp::Equal => vk::CompareOp::EQUAL,
        CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareOp::Greater => vk::CompareOp::GREATER,
        CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
        CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareOp::Always => vk::CompareOp::ALWAYS,
    }
}

#[inline(always)]
pub(crate) fn to_vk_blend_factor(bf: BlendFactor) -> vk::BlendFactor {
    match bf {
        BlendFactor::Zero => vk::BlendFactor::ZERO,
        BlendFactor::One => vk::BlendFactor::ONE,
        BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
        BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
        BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
        BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
    }
}

#[inline(always)]
pub(crate) fn to_vk_blend_op(bo: BlendOp) -> vk::BlendOp {
    match bo {
        BlendOp::Add => vk::BlendOp::ADD,
        BlendOp::Subtract => vk::BlendOp::SUBTRACT,
        BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
        BlendOp::Min => vk::BlendOp::MIN,
        BlendOp::Max => vk::BlendOp::MAX,
    }
}

#[inline(always)]
pub(crate) fn to_vk_filter(f: Filter) -> vk::Filter {
    match f {
        Filter::Nearest => vk::Filter::NEAREST,
        Filter::Linear => vk::Filter::LINEAR,
    }
}

#[inline(always)]
pub(crate) fn to_vk_reduction_mode(rm: ReductionMode) -> vk::SamplerReductionMode {
    match rm {
        ReductionMode::Min => vk::SamplerReductionMode::MIN,
        ReductionMode::Max => vk::SamplerReductionMode::MAX,
    }
}

#[inline(always)]
pub(crate) fn to_vk_address_mode(sam: SamplerAddressMode) -> vk::SamplerAddressMode {
    match sam {
        SamplerAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        SamplerAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        SamplerAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        SamplerAddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    }
}

#[inline(always)]
pub(crate) fn to_vk_border_color(bc: BorderColor) -> vk::BorderColor {
    match bc {
        BorderColor::FloatTransparentBlack => vk::BorderColor::FLOAT_TRANSPARENT_BLACK,
        BorderColor::IntTransparentBlack => vk::BorderColor::INT_TRANSPARENT_BLACK,
        BorderColor::FloatOpaqueBlack => vk::BorderColor::FLOAT_OPAQUE_BLACK,
        BorderColor::IntOpaqueBlack => vk::BorderColor::INT_OPAQUE_BLACK,
        BorderColor::FloatOpaqueWhite => vk::BorderColor::FLOAT_OPAQUE_WHITE,
        BorderColor::IntOpaqueWhite => vk::BorderColor::INT_OPAQUE_WHITE,
    }
}

#[inline(always)]
pub(crate) fn to_vk_color_components(cc: ColorComponents) -> vk::ColorComponentFlags {
    let mut out = vk::ColorComponentFlags::default();
    if cc.contains(ColorComponents::R) {
        out |= vk::ColorComponentFlags::R;
    }
    if cc.contains(ColorComponents::G) {
        out |= vk::ColorComponentFlags::G;
    }
    if cc.contains(ColorComponents::B) {
        out |= vk::ColorComponentFlags::B;
    }
    if cc.contains(ColorComponents::A) {
        out |= vk::ColorComponentFlags::A;
    }
    out
}

#[inline(always)]
pub(crate) fn to_vk_buffer_usage(bu: BufferUsage) -> vk::BufferUsageFlags {
    let mut out = vk::BufferUsageFlags::default();
    if bu.contains(BufferUsage::INDEX_BUFFER) {
        out |= vk::BufferUsageFlags::INDEX_BUFFER;
    }
    if bu.contains(BufferUsage::VERTEX_BUFFER) {
        out |= vk::BufferUsageFlags::VERTEX_BUFFER;
    }
    if bu.contains(BufferUsage::UNIFORM_BUFFER) {
        out |= vk::BufferUsageFlags::UNIFORM_BUFFER;
    }
    if bu.contains(BufferUsage::STORAGE_BUFFER) {
        out |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if bu.contains(BufferUsage::TRANSFER_DST) {
        out |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    if bu.contains(BufferUsage::TRANSFER_SRC) {
        out |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if bu.contains(BufferUsage::INDIRECT_BUFFER) {
        out |= vk::BufferUsageFlags::INDIRECT_BUFFER;
    }
    out
}

#[inline(always)]
pub(crate) fn to_vk_image_usage(iu: TextureUsage) -> vk::ImageUsageFlags {
    let mut out = vk::ImageUsageFlags::default();
    if iu.contains(TextureUsage::TRANSFER_SRC) {
        out |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if iu.contains(TextureUsage::TRANSFER_DST) {
        out |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    if iu.contains(TextureUsage::SAMPLED) {
        out |= vk::ImageUsageFlags::SAMPLED;
    }
    if iu.contains(TextureUsage::STORAGE) {
        out |= vk::ImageUsageFlags::STORAGE;
    }
    if iu.contains(TextureUsage::COLOR_ATTACHMENT) {
        out |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
    }
    if iu.contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT) {
        out |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
    }
    out
}

#[inline(always)]
pub(crate) fn to_vk_image_type(it: TextureType) -> vk::ImageType {
    match it {
        TextureType::Type1D => vk::ImageType::TYPE_1D,
        TextureType::Type2D => vk::ImageType::TYPE_2D,
        TextureType::Type3D => vk::ImageType::TYPE_3D,
    }
}

#[inline(always)]
pub(crate) fn to_gpu_allocator_memory_location(mu: MemoryUsage) -> MemoryLocation {
    match mu {
        MemoryUsage::Unknown => MemoryLocation::Unknown,
        MemoryUsage::GpuOnly => MemoryLocation::GpuOnly,
        MemoryUsage::CpuToGpu => MemoryLocation::CpuToGpu,
        MemoryUsage::GpuToCpu => MemoryLocation::GpuToCpu,
    }
}
