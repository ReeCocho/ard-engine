use ard_formats::vertex::VertexLayout;
use ard_pal::prelude::ShaderStage;
use serde::{Deserialize, Serialize};

use crate::RenderingMode;

/// Shader variant used to identify a compiled shader.
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShaderVariant {
    pub pass: usize,
    pub vertex_layout: VertexLayout,
    pub stage: ShaderStage,
    pub rendering_mode: RenderingMode,
}
