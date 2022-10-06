use ard_math::Vec4;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub const PBR_TEXTURE_COUNT: usize = 3;

pub const PBR_DIFFUSE_MAP_SLOT: usize = 0;
pub const PBR_NORMAL_MAP_SLOT: usize = 1;
pub const PBR_METALLIC_ROUGHNESS_MAP_SLOT: usize = 2;

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct PbrMaterial {
    pub base_color: Vec4,
    pub metallic: f32,
    pub roughness: f32,
    pub alpha_cutoff: f32,
}

unsafe impl Pod for PbrMaterial {}
unsafe impl Zeroable for PbrMaterial {}
