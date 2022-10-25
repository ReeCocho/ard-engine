use ard_pal::prelude::{Filter, SamplerAddressMode, TextureFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureHeader {
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub format: TextureFormat,
    pub sampler: Sampler,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mipmap_filter: Filter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
}
