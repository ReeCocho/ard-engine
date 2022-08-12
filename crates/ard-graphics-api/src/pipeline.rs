use crate::Backend;

pub struct PipelineCreateInfo<B: Backend> {
    pub vertex: B::Shader,
    pub fragment: B::Shader,
    pub use_occlusion_culling: bool,
    pub use_depth_buffer: bool,
}

pub trait PipelineApi: Clone + Send + Sync {}
