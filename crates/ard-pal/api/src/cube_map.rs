use crate::{context::Context, types::*, Backend};

use thiserror::*;

pub struct CubeMapCreateInfo {
    pub format: TextureFormat,
    pub size: u32,
    pub array_elements: usize,
    pub mip_levels: usize,
    pub texture_usage: TextureUsage,
    pub memory_usage: MemoryUsage,
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum CubeMapCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

pub struct CubeMap<B: Backend> {
    ctx: Context<B>,
    size: u32,
    mip_count: usize,
    pub(crate) id: B::CubeMap,
}

impl<B: Backend> CubeMap<B> {
    pub fn new(
        ctx: Context<B>,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self, CubeMapCreateError> {
        let size = create_info.size;
        let mip_count = create_info.mip_levels;
        let id = unsafe { ctx.0.create_cube_map(create_info)? };
        Ok(Self {
            ctx,
            size,
            mip_count,
            id,
        })
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::CubeMap {
        &self.id
    }

    #[inline(always)]
    pub fn size(&self) -> u32 {
        self.size
    }

    #[inline(always)]
    pub fn mip_count(&self) -> usize {
        self.mip_count
    }
}

impl<B: Backend> Drop for CubeMap<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_cube_map(&mut self.id);
        }
    }
}

impl Default for CubeMapCreateInfo {
    #[inline(always)]
    fn default() -> Self {
        Self {
            format: TextureFormat::Rgba8Unorm,
            size: 128,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::empty(),
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: None,
        }
    }
}
