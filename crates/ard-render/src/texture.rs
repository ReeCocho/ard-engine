use ard_pal::prelude::{
    Buffer, Context, Filter, MemoryUsage, SamplerAddressMode, TextureFormat, TextureType,
    TextureUsage,
};

use crate::factory::allocator::{EscapeHandle, ResourceId};

pub struct TextureCreateInfo<'a> {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: &'a [u8],
    pub mip_type: MipType,
    pub mip_count: usize,
    pub sampler: Sampler,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mipmap_filter: Filter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
    pub anisotropy: bool,
}

/// Indicates how a texture should get it's mip levels.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MipType {
    /// Mip maps will be autogenerated from the image data.
    Generate,
    /// Data contains only the highest level mip. Other mip levels will be provided later.
    Upload,
}

#[derive(Clone)]
pub struct Texture {
    pub(crate) id: ResourceId,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct TextureInner {
    pub texture: ard_pal::prelude::Texture,
    pub sampler: Sampler,
    /// Bit mask that indicates which mip levels of the texture are loaded into memory. The least
    /// significant bit represents LOD0 (the highest detail image).
    pub mip_levels: u32,
    pub loaded_mips: u32,
}

impl TextureInner {
    /// Creates the texture and a staging buffer with the image data.
    pub fn new(ctx: &Context, create_info: TextureCreateInfo) -> (Self, Buffer) {
        let mip_levels = (create_info.width.max(create_info.height) as f32)
            .log2()
            .floor() as usize
            + 1;
        assert!(create_info.mip_count <= mip_levels);

        // Create the texture
        let texture = ard_pal::prelude::Texture::new(
            ctx.clone(),
            ard_pal::prelude::TextureCreateInfo {
                format: create_info.format,
                ty: TextureType::Type2D,
                width: create_info.width,
                height: create_info.height,
                depth: 1,
                array_elements: 1,
                mip_levels: create_info.mip_count,
                texture_usage: TextureUsage::TRANSFER_SRC
                    | TextureUsage::TRANSFER_DST
                    | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: None,
            },
        )
        .unwrap();

        // Create the staging buffer for the texture
        let staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("texture_staging")),
            create_info.data,
        )
        .unwrap();

        let loaded_mips = match create_info.mip_type {
            // All mips will be available when the texture is ready
            MipType::Generate => (1 << create_info.mip_count) - 1,
            // Only the lowest detail mip is loaded.
            MipType::Upload => (1 << (create_info.mip_count - 1)),
        };

        (
            Self {
                texture,
                sampler: create_info.sampler,
                mip_levels: create_info.mip_count as u32,
                loaded_mips,
            },
            staging,
        )
    }

    /// Determine the base mip and number of mips.
    #[inline(always)]
    pub fn loaded_mips(&self) -> (u32, u32) {
        let loaded_mips = self.loaded_mips << (u32::BITS - self.mip_levels);
        let lz = loaded_mips.leading_zeros();
        let loaded_mips = (loaded_mips << lz).leading_ones();
        let base_mip_level = self.mip_levels - (lz + loaded_mips);
        (base_mip_level, loaded_mips)
    }
}