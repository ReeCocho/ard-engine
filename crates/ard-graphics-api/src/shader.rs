use crate::mesh::VertexLayout;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderType {
    Fragment,
    Vertex,
}

#[derive(Debug)]
pub struct ShaderCreateInfo<'a> {
    pub ty: ShaderType,
    pub vertex_layout: VertexLayout,
    pub inputs: ShaderInputs,
    pub code: &'a [u8],
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ShaderInputs {
    pub ubo_size: u64,
    pub texture_count: usize,
}

pub trait ShaderApi: Clone + Send + Sync {
    fn ty(&self) -> ShaderType;

    fn inputs(&self) -> &ShaderInputs;
}
