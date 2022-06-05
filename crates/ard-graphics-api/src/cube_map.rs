use crate::{prelude::SamplerDescriptor, TextureFormat};

pub struct CubeMapCreateInfo<'a> {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: &'a [u8],
    pub sampler: SamplerDescriptor,
}

pub trait CubeMapApi: Clone + Send + Sync {}
