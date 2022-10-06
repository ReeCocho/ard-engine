use ard_pal::prelude::{Buffer, Context, MemoryUsage, TextureFormat, TextureUsage};

use crate::{
    factory::allocator::{EscapeHandle, ResourceId},
    texture::{MipType, Sampler},
};

pub struct CubeMapCreateInfo<'a> {
    pub size: u32,
    pub format: TextureFormat,
    pub data: &'a [u8],
    pub mip_type: MipType,
    pub mip_count: usize,
    pub sampler: Sampler,
}

#[derive(Clone)]
pub struct CubeMap {
    pub(crate) id: ResourceId,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CubeMapInner {
    pub cube_map: ard_pal::prelude::CubeMap,
    pub sampler: Sampler,
    /// Bit mask that indicates which mip levels of the texture are loaded into memory. The least
    /// significant bit represents LOD0 (the highest detail image).
    pub mip_levels: u32,
    pub loaded_mips: u32,
}

impl CubeMapInner {
    pub fn new(ctx: &Context, create_info: CubeMapCreateInfo) -> (Self, Buffer) {
        let mip_levels = (create_info.size as f32).log2().floor() as usize + 1;
        assert!(create_info.mip_count <= mip_levels);

        // Create the cube map
        let cube_map = ard_pal::prelude::CubeMap::new(
            ctx.clone(),
            ard_pal::prelude::CubeMapCreateInfo {
                format: create_info.format,
                size: create_info.size,
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

        // Create the staging buffer for the cube map
        let staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("cube_map_staging")),
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
                cube_map,
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
