use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureFormat {
    R8Unorm,
    R32Sfloat,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba8Srgb,
    Bgra8Unorm,
    Bgra8Srgb,
    Rgba16SFloat,
    Rg16SFloat,
    R16SFloat,
    BC7Srgb,
    BC7Unorm,
    D16Unorm,
    D24UnormS8Uint,
    D32Sfloat,
    D32SfloatS8Uint,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VertexFormat {
    XF32,
    XyF32,
    XyzwF32,
    XyzwU8,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexType {
    U16,
    U32,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VertexInputRate {
    Vertex,
    Instance,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrimitiveTopology {
    PontList,
    LineList,
    TriangleList,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PolygonMode {
    Fill,
    Line,
    Point,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CullMode {
    None,
    Front,
    Back,
    FrontAndBack,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FrontFace {
    CounterClockwise,
    Clockwise,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompareOp {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

bitflags! {
    pub struct ColorComponents: u32 {
        const R = 0b0001;
        const G = 0b0010;
        const B = 0b0100;
        const A = 0b1000;
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JobStatus {
    /// The job is still running.
    Running,
    /// The job is complete.
    Complete,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QueueType {
    /// The main queue is guaranteed to support graphics, transfer, and compute operations.
    Main,
    /// The transfer queue is guaranteed to support transfer operations and usually operates
    /// asynchronously to other queues.
    Transfer,
    /// The transfer queue is guaranteed to support compute operations and usually operates
    /// asynchronously to other queues.
    Compute,
    /// The transfer queue is guaranteed to support surface presentation.
    Present,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StoreOp {
    /// We don't care what happens to the contents of the image after the pass.
    DontCare,
    /// The contents of the image should be stored after the pass.
    Store,
}

#[derive(Debug, Copy, Clone)]
pub enum LoadOp {
    /// We don't care about the contents of the image.
    DontCare,
    /// The contents of the image should be loaded.
    Load,
    /// The contents of the image should be cleared with the specified color.
    Clear(ClearColor),
}

#[derive(Debug, Copy, Clone)]
pub enum ClearColor {
    RgbaF32(f32, f32, f32, f32),
    RU32(u32),
    D32S32(f32, u32),
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PresentMode {
    /// The presentation engine will not wait for a vertical blanking period to update the current
    /// image. Visisble tearing may occur.
    Immediate,
    /// The presentation engine will wait for a vertical blanking period to update the image,
    /// pulling from a single-entry queue which contains the next image to present. If a new image
    /// is sent for presentation, the old image will be discarded. Visible tearing will not occur.
    Mailbox,
    /// The presentation engine will wait for a vertical blanking period to update the image,
    /// pulling from a fifo-queue which contains images to present. If a new image is sent for
    /// presentation, it will be appended to the queue. Visible tearing will not occur.
    Fifo,
    /// The presentation engine will generally wait for a vertical blanking period to update the
    /// image. However, if a vertical blanking period has passed since the lat update of the
    /// current image, then the presentation engine will not wait for another vertical blanking
    /// period. Visible tearing will occur if images are not submitted at least as fast as the
    /// vertical blanking period.
    FifoRelaxed,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderStage {
    AllGraphics,
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReductionMode {
    Min,
    Max,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SamplerAddressMode {
    Repeat,
    MirroredRepeat,
    ClampToEdge,
    ClampToBorder,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BorderColor {
    FloatTransparentBlack,
    IntTransparentBlack,
    FloatOpaqueBlack,
    IntOpaqueBlack,
    FloatOpaqueWhite,
    IntOpaqueWhite,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnisotropyLevel {
    X1,
    X2,
    X4,
    X8,
    X16,
}

bitflags! {
    pub struct BufferUsage: u32 {
        const TRANSFER_SRC    = 0b0000001;
        const TRANSFER_DST    = 0b0000010;
        const UNIFORM_BUFFER  = 0b0000100;
        const STORAGE_BUFFER  = 0b0001000;
        const VERTEX_BUFFER   = 0b0010000;
        const INDEX_BUFFER    = 0b0100000;
        const INDIRECT_BUFFER = 0b1000000;
    }
}

bitflags! {
    pub struct TextureUsage: u32 {
        const TRANSFER_SRC             = 0b0000001;
        const TRANSFER_DST             = 0b0000010;
        const SAMPLED                  = 0b0000100;
        const STORAGE                  = 0b0001000;
        const COLOR_ATTACHMENT         = 0b0010000;
        const DEPTH_STENCIL_ATTACHMENT = 0b0100000;
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureType {
    Type1D,
    Type2D,
    Type3D,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryUsage {
    Unknown,
    GpuOnly,
    CpuToGpu,
    GpuToCpu,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AccessType {
    Read,
    ReadWrite,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scissor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl TextureFormat {
    #[inline(always)]
    pub fn is_color(&self) -> bool {
        !(self.is_depth() || self.is_stencil())
    }

    #[inline(always)]
    pub fn is_depth(&self) -> bool {
        match *self {
            TextureFormat::D16Unorm
            | TextureFormat::D24UnormS8Uint
            | TextureFormat::D32Sfloat
            | TextureFormat::D32SfloatS8Uint => true,
            _ => false,
        }
    }

    #[inline(always)]
    pub fn is_stencil(&self) -> bool {
        match *self {
            TextureFormat::D24UnormS8Uint | TextureFormat::D32SfloatS8Uint => true,
            _ => false,
        }
    }
}
