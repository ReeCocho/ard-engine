use std::sync::Arc;

use crate::{
    context::Context, descriptor_set::DescriptorSetLayout, shader::Shader, types::ShaderStage,
    Backend,
};
use thiserror::*;

#[derive(Clone)]
pub struct RayTracingPipelineCreateInfo<B: Backend> {
    pub stages: Vec<RayTracingShaderStage<B>>,
    pub groups: Vec<RayTracingShaderGroup>,
    pub max_ray_recursion_depth: u32,
    pub layouts: Vec<DescriptorSetLayout<B>>,
    pub push_constants_size: Option<u32>,
    pub debug_name: Option<String>,
}

#[derive(Clone)]
pub struct RayTracingShaderStage<B: Backend> {
    pub shader: Shader<B>,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RayTracingShaderGroup {
    RayGeneration(usize),
    Miss(usize),
    Triangles {
        closest_hit: Option<usize>,
        any_hit: Option<usize>,
    },
}

pub struct ShaderBindingTableData {
    /// Raw binding table data from the API. Size is `entry_count * entry_size`.
    pub raw: Vec<u8>,
    /// The number of table entries retrieved.
    pub entry_count: usize,
    /// The size of each element in the raw buffer.
    pub entry_size: u64,
    /// The size required by the API for entries in a SBT.
    pub aligned_size: u64,
    /// The alignment required for the beginning of SBT tables.
    pub base_alignment: u64,
}

pub struct RayTracingPipeline<B: Backend>(pub(crate) Arc<RayTracingPipelineInner<B>>);

pub(crate) struct RayTracingPipelineInner<B: Backend> {
    ctx: Context<B>,
    pub(crate) layouts: Vec<DescriptorSetLayout<B>>,
    pub(crate) id: B::RayTracingPipeline,
}

#[derive(Debug, Error)]
pub enum RayTracingPipelineCreateError {
    #[error("an error occured: {0}")]
    Other(String),
}

impl<B: Backend> RayTracingPipeline<B> {
    pub fn new(
        ctx: Context<B>,
        create_info: RayTracingPipelineCreateInfo<B>,
    ) -> Result<Self, RayTracingPipelineCreateError> {
        let layouts = create_info.layouts.clone();
        let id = unsafe { ctx.0.create_ray_tracing_pipeline(create_info)? };
        Ok(Self(Arc::new(RayTracingPipelineInner { ctx, id, layouts })))
    }

    #[inline(always)]
    pub fn shader_binding_table_data(&self) -> ShaderBindingTableData {
        unsafe { self.0.ctx.0.shader_binding_table_data(&self.0.id) }
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::RayTracingPipeline {
        &self.0.id
    }

    #[inline(always)]
    pub fn layouts(&self) -> &[DescriptorSetLayout<B>] {
        &self.0.layouts
    }
}

impl<B: Backend> Clone for RayTracingPipeline<B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<B: Backend> Drop for RayTracingPipelineInner<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_ray_tracing_pipeline(&mut self.id);
        }
    }
}
