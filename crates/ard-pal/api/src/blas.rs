use thiserror::Error;

use crate::{
    buffer::Buffer,
    context::Context,
    types::{
        BuildAccelerationStructureFlags, Format, GeometryFlags, IndexType, QueueTypes, SharingMode,
    },
    Backend,
};

pub enum BottomLevelAccelerationStructureData<'a, B: Backend> {
    Geometry(Vec<AccelerationStructureGeometry<'a, B>>),
    CompactDst(u64),
}

pub struct AccelerationStructureGeometry<'a, B: Backend> {
    pub flags: GeometryFlags,
    pub vertex_format: Format,
    pub vertex_data: &'a Buffer<B>,
    pub vertex_data_array_element: usize,
    pub vertex_data_offset: u64,
    pub vertex_count: usize,
    pub vertex_stride: u64,
    pub index_type: IndexType,
    pub index_data: &'a Buffer<B>,
    pub index_data_array_element: usize,
    pub index_data_offset: u64,
    pub triangle_count: usize,
}

pub struct BottomLevelAccelerationStructureCreateInfo<'a, B: Backend> {
    pub flags: BuildAccelerationStructureFlags,
    pub data: BottomLevelAccelerationStructureData<'a, B>,
    pub queue_types: QueueTypes,
    pub sharing_mode: SharingMode,
    pub debug_name: Option<String>,
}

pub struct BottomLevelAccelerationStructure<B: Backend> {
    ctx: Context<B>,
    id: B::BottomLevelAccelerationStructure,
}

#[derive(Debug, Error)]
pub enum BottomLevelAccelerationStructureCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

impl<B: Backend> BottomLevelAccelerationStructure<B> {
    pub fn new<'a>(
        ctx: Context<B>,
        create_info: BottomLevelAccelerationStructureCreateInfo<'a, B>,
    ) -> Result<Self, BottomLevelAccelerationStructureCreateError> {
        let id = unsafe {
            ctx.0
                .create_bottom_level_acceleration_structure(create_info)?
        };

        Ok(Self { ctx, id })
    }

    #[inline(always)]
    pub fn device_ref(&self) -> u64 {
        unsafe { self.ctx.0.blas_device_ref(&self.id) }
    }

    #[inline(always)]
    pub fn scratch_buffer_size(&self) -> u64 {
        unsafe { self.ctx.0.blas_scratch_size(&self.id) }
    }

    #[inline(always)]
    pub fn compacted_size(&self) -> u64 {
        unsafe { self.ctx.0.blas_compacted_size(&self.id) }
    }

    #[inline(always)]
    pub fn build_flags(&self) -> BuildAccelerationStructureFlags {
        unsafe { self.ctx.0.blas_build_flags(&self.id) }
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::BottomLevelAccelerationStructure {
        &self.id
    }
}

impl<B: Backend> Drop for BottomLevelAccelerationStructure<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .destroy_bottom_level_acceleration_structure(&mut self.id);
        }
    }
}
