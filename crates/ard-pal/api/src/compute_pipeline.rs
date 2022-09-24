use crate::{context::Context, descriptor_set::DescriptorSetLayout, shader::Shader, Backend};
use std::sync::Arc;
use thiserror::Error;

pub struct ComputePipelineCreateInfo<B: Backend> {
    /// Layouts for sets that are to be bound to the pipeline.
    pub layouts: Vec<DescriptorSetLayout<B>>,
    /// Shader module of the pipeline.
    pub module: Shader<B>,
    /// The size of each dispatched work group.
    pub work_group_size: (u32, u32, u32),
    /// Number of bytes to use for push constants.
    pub push_constants_size: Option<u32>,
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum ComputePipelineCreateError {
    #[error("an error occured: {0}")]
    Other(String),
}

pub struct ComputePipeline<B: Backend>(Arc<ComputePipelineInner<B>>);

pub(crate) struct ComputePipelineInner<B: Backend> {
    ctx: Context<B>,
    pub(crate) layouts: Vec<DescriptorSetLayout<B>>,
    pub(crate) id: B::ComputePipeline,
}

impl<B: Backend> ComputePipeline<B> {
    /// Creates a new compute pipeline.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `create_info` - Describes the compute pipeline to create.
    ///
    /// # Panics
    /// - If any element of `work_group_size` is `0`.
    pub fn new(
        ctx: Context<B>,
        create_info: ComputePipelineCreateInfo<B>,
    ) -> Result<Self, ComputePipelineCreateError> {
        assert_ne!(create_info.work_group_size.0, 0, "work group size x is 0");
        assert_ne!(create_info.work_group_size.1, 0, "work group size y is 0");
        assert_ne!(create_info.work_group_size.2, 0, "work group size z is 0");

        let layouts = create_info.layouts.clone();
        let id = unsafe { ctx.0.create_compute_pipeline(create_info)? };
        Ok(Self(Arc::new(ComputePipelineInner { ctx, id, layouts })))
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::ComputePipeline {
        &self.0.id
    }

    #[inline(always)]
    pub fn layouts(&self) -> &[DescriptorSetLayout<B>] {
        &self.0.layouts
    }
}

impl<B: Backend> Drop for ComputePipelineInner<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_compute_pipeline(&mut self.id);
        }
    }
}

impl<B: Backend> Clone for ComputePipeline<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
