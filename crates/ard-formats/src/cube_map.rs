use ard_pal::prelude::Format;
use serde::{Deserialize, Serialize};

use crate::texture::Sampler;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CubeMapHeader {
    pub size: u32,
    pub mip_count: u32,
    pub format: Format,
    pub sampler: Sampler,
}
