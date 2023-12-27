use ard_formats::texture::{MipType, Sampler, TextureSource};
use ard_pal::prelude::{
    Buffer, BufferCreateError, Context, MemoryUsage, QueueType, QueueTypes, SharingMode,
    TextureType, TextureUsage,
};
use ard_render_base::resource::{ResourceHandle, ResourceId};
use thiserror::*;

use crate::factory::TextureUpload;

type PalTexture = ard_pal::prelude::Texture;
type PalTextureCreateInfo = ard_pal::prelude::TextureCreateInfo;
type PalTextureCreateError = ard_pal::prelude::TextureCreateError;

pub struct TextureCreateInfo<T> {
    pub source: T,
    pub debug_name: Option<String>,
    pub mip_count: usize,
    pub mip_type: MipType,
    pub sampler: Sampler,
}

#[derive(Debug, Error)]
pub enum TextureCreateError<T: TextureSource> {
    #[error("Invalid mip count. Mip count must be greater than 0 and less than or equal to {0} for the provided texture data.")]
    InvalidMipCount(usize),
    #[error("texture data error: {0}")]
    TextureDataErr(T::Error),
    #[error("buffer error: {0}")]
    BufferError(BufferCreateError),
    #[error("internal texture error: {0}")]
    TextureError(ard_pal::prelude::TextureCreateError),
}

#[derive(Clone)]
pub struct Texture {
    handle: ResourceHandle,
}

pub struct TextureResource {
    pub texture: PalTexture,
    pub sampler: Sampler,
    pub mip_levels: u32,
    /// Bit mask that indicates which mip levels of the texture are loaded into memory. The least
    /// significant bit represents LOD0 (the highest detail image).
    pub loaded_mips: u32,
}

impl Texture {
    pub fn new(handle: ResourceHandle) -> Self {
        Texture { handle }
    }

    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.handle.id()
    }
}

impl TextureResource {
    pub fn new<T: TextureSource>(
        ctx: &Context,
        create_info: TextureCreateInfo<T>,
    ) -> Result<(Self, TextureUpload), TextureCreateError<T>> {
        let data = match create_info.source.into_texture_data() {
            Ok(data) => data,
            Err(err) => return Err(TextureCreateError::TextureDataErr(err)),
        };

        let (width, height) = match create_info.mip_type {
            MipType::Generate => (data.width(), data.height()),
            MipType::Upload(w, h) => (w, h),
        };

        let max_mip_levels = (width.max(height) as f32).log2().floor() as usize + 1;

        if create_info.mip_count > max_mip_levels {
            return Err(TextureCreateError::InvalidMipCount(max_mip_levels));
        }

        let texture = PalTexture::new(
            ctx.clone(),
            PalTextureCreateInfo {
                format: data.format(),
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 1,
                mip_levels: create_info.mip_count,
                texture_usage: TextureUsage::TRANSFER_SRC
                    | TextureUsage::TRANSFER_DST
                    | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Exclusive,
                debug_name: create_info.debug_name,
            },
        )?;

        let (loaded_mips, staging_queue): (u32, QueueType) = match create_info.mip_type {
            // All mips will be available when the texture is ready
            MipType::Generate => ((1 << create_info.mip_count as u32) - 1, QueueType::Main),
            // Only the lowest detail mip is loaded.
            MipType::Upload(_, _) => (1 << (create_info.mip_count as u32 - 1), QueueType::Transfer),
        };

        let staging = Buffer::new_staging(
            ctx.clone(),
            staging_queue,
            Some("texture_staging".to_owned()),
            data.raw(),
        )?;

        Ok((
            Self {
                texture,
                sampler: create_info.sampler,
                mip_levels: create_info.mip_count as u32,
                loaded_mips,
            },
            TextureUpload {
                staging,
                mip_type: create_info.mip_type,
            },
        ))
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

impl<T: TextureSource> From<PalTextureCreateError> for TextureCreateError<T> {
    fn from(value: PalTextureCreateError) -> Self {
        TextureCreateError::TextureError(value)
    }
}

impl<T: TextureSource> From<BufferCreateError> for TextureCreateError<T> {
    fn from(value: BufferCreateError) -> Self {
        TextureCreateError::BufferError(value)
    }
}
