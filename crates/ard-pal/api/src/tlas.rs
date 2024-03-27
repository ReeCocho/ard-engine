use thiserror::*;

use crate::{
    context::Context,
    types::{BuildAccelerationStructureFlags, QueueTypes, SharingMode},
    Backend,
};

pub struct TopLevelAccelerationStructureCreateInfo {
    pub flags: BuildAccelerationStructureFlags,
    pub capacity: usize,
    pub queue_types: QueueTypes,
    pub sharing_mode: SharingMode,
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum TopLevelAccelerationStructureCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

pub struct TopLevelAccelerationStructure<B: Backend> {
    ctx: Context<B>,
    id: B::TopLevelAccelerationStructure,
}

impl<B: Backend> TopLevelAccelerationStructure<B> {
    pub fn new<'a>(
        ctx: Context<B>,
        create_info: TopLevelAccelerationStructureCreateInfo,
    ) -> Result<Self, TopLevelAccelerationStructureCreateError> {
        let id = unsafe { ctx.0.create_top_level_acceleration_structure(create_info)? };

        Ok(Self { ctx, id })
    }

    #[inline(always)]
    pub fn scratch_buffer_size(&self) -> u64 {
        unsafe { self.ctx.0.tlas_scratch_size(&self.id) }
    }

    #[inline(always)]
    pub fn build_flags(&self) -> BuildAccelerationStructureFlags {
        unsafe { self.ctx.0.tlas_build_flags(&self.id) }
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::TopLevelAccelerationStructure {
        &self.id
    }
}

impl<B: Backend> Drop for TopLevelAccelerationStructure<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .destroy_top_level_acceleration_structure(&mut self.id);
        }
    }
}
