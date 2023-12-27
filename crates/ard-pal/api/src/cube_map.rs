use crate::{context::Context, types::*, Backend};

use thiserror::*;

pub struct CubeMapCreateInfo {
    pub format: Format,
    pub size: u32,
    pub array_elements: usize,
    pub mip_levels: usize,
    pub texture_usage: TextureUsage,
    pub memory_usage: MemoryUsage,
    pub queue_types: QueueTypes,
    pub sharing_mode: SharingMode,
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum CubeMapCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

pub struct CubeMap<B: Backend> {
    ctx: Context<B>,
    dim: u32,
    mip_count: usize,
    queue_types: QueueTypes,
    sharing_mode: SharingMode,
    pub(crate) id: B::CubeMap,
}

impl<B: Backend> CubeMap<B> {
    pub fn new(
        ctx: Context<B>,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self, CubeMapCreateError> {
        let size = create_info.size;
        let mip_count = create_info.mip_levels;
        let queue_types = create_info.queue_types;
        let sharing_mode = create_info.sharing_mode;
        let id = unsafe { ctx.0.create_cube_map(create_info)? };
        Ok(Self {
            ctx,
            dim: size,
            mip_count,
            queue_types,
            sharing_mode,
            id,
        })
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::CubeMap {
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
    pub fn dim(&self) -> u32 {
        self.dim
    }

    /// Gets the size in bytes of a single array element of the cube map.
    #[inline(always)]
    pub fn size(&self) -> u64 {
        unsafe { self.ctx.0.cube_map_size(&self.id) }
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
            format: Format::Rgba8Unorm,
            size: 128,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::empty(),
            memory_usage: MemoryUsage::GpuOnly,
            queue_types: QueueTypes::all(),
            sharing_mode: SharingMode::Concurrent,
            debug_name: None,
        }
    }
}
