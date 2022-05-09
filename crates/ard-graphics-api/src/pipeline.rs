use crate::Backend;

pub struct PipelineCreateInfo<B: Backend> {
    pub vertex: B::Shader,
    pub fragment: B::Shader,
}

pub trait PipelineApi: Clone + Send + Sync {}
