use ard_math::Vec4;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MaterialHeader<TextureRef> {
    pub blend_ty: BlendType,
    pub ty: MaterialType<TextureRef>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum MaterialType<TextureRef> {
    Pbr {
        base_color: Vec4,
        metallic: f32,
        roughness: f32,
        alpha_cutoff: f32,
        diffuse_map: Option<TextureRef>,
        normal_map: Option<TextureRef>,
        metallic_roughness_map: Option<TextureRef>,
    },
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlendType {
    Opaque,
    Mask,
    Blend,
}
