use crate::{
    prelude::{MipType, SamplerDescriptor},
    TextureFormat,
};

pub struct CubeMapCreateInfo<'a> {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: &'a [u8],
    pub mip_type: MipType,
    pub mip_count: usize,
    pub sampler: SamplerDescriptor,
}

pub trait CubeMapApi: Clone + Send + Sync {}
