use crate::{
    context::Context,
    types::{
        AnisotropyLevel, BorderColor, CompareOp, Filter, Format, MemoryUsage, MultiSamples,
        QueueTypes, SamplerAddressMode, SharingMode, TextureType, TextureUsage,
    },
    Backend,
};
use ordered_float::NotNan;
use thiserror::Error;

pub struct TextureCreateInfo {
    pub format: Format,
    pub ty: TextureType,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_elements: usize,
    pub mip_levels: usize,
    pub sample_count: MultiSamples,
    pub texture_usage: TextureUsage,
    pub memory_usage: MemoryUsage,
    pub queue_types: QueueTypes,
    pub sharing_mode: SharingMode,
    pub debug_name: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mipmap_filter: Filter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
    pub address_w: SamplerAddressMode,
    pub anisotropy: Option<AnisotropyLevel>,
    pub compare: Option<CompareOp>,
    pub min_lod: NotNan<f32>,
    pub max_lod: Option<NotNan<f32>>,
    pub border_color: Option<BorderColor>,
    pub unnormalize_coords: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Blit {
    pub src_min: (u32, u32, u32),
    pub src_max: (u32, u32, u32),
    pub src_mip: usize,
    pub src_array_element: usize,
    pub dst_min: (u32, u32, u32),
    pub dst_max: (u32, u32, u32),
    pub dst_mip: usize,
    pub dst_array_element: usize,
}

#[derive(Debug, Error)]
pub enum TextureCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

pub struct Texture<B: Backend> {
    ctx: Context<B>,
    dims: (u32, u32, u32),
    format: Format,
    mip_count: usize,
    queue_types: QueueTypes,
    sharing_mode: SharingMode,
    pub(crate) id: B::Texture,
}

impl<B: Backend> Texture<B> {
    pub fn new(
        ctx: Context<B>,
        create_info: TextureCreateInfo,
    ) -> Result<Self, TextureCreateError> {
        let dims = (create_info.width, create_info.height, create_info.depth);
        let mip_count = create_info.mip_levels;
        let format = create_info.format;
        let queue_types = create_info.queue_types;
        let sharing_mode = create_info.sharing_mode;
        let id = unsafe { ctx.0.create_texture(create_info)? };

        Ok(Self {
            ctx,
            dims,
            id,
            format,
            queue_types,
            sharing_mode,
            mip_count,
        })
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::Texture {
        &self.id
    }

    #[inline(always)]
    pub fn queue_types(&self) -> QueueTypes {
        self.queue_types
    }

    #[inline(always)]
    pub fn sharing_mode(&self) -> SharingMode {
        self.sharing_mode
    }

    #[inline(always)]
    pub fn dims(&self) -> (u32, u32, u32) {
        self.dims
    }

    #[inline(always)]
    pub fn format(&self) -> Format {
        self.format
    }

    /// Gets the size in bytes of a single array element of the texture.
    #[inline(always)]
    pub fn size(&self) -> u64 {
        unsafe { self.ctx.0.texture_size(&self.id) }
    }

    #[inline(always)]
    pub fn mip_count(&self) -> usize {
        self.mip_count
    }
}

impl<B: Backend> Drop for Texture<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_texture(&mut self.id);
        }
    }
}

impl Default for TextureCreateInfo {
    #[inline(always)]
    fn default() -> Self {
        Self {
            format: Format::Rgba8Unorm,
            ty: TextureType::Type2D,
            width: 128,
            height: 128,
            depth: 1,
            array_elements: 1,
            mip_levels: 1,
            sample_count: MultiSamples::Count1,
            texture_usage: TextureUsage::empty(),
            memory_usage: MemoryUsage::GpuOnly,
            queue_types: QueueTypes::all(),
            sharing_mode: SharingMode::Concurrent,
            debug_name: None,
        }
    }
}
