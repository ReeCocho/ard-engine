use std::error::Error;

use ard_assets::asset::AssetNameBuf;
use ard_pal::prelude::{Filter, Format, SamplerAddressMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureHeader {
    pub mips: Vec<AssetNameBuf>,
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub sampler: Sampler,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mipmap_filter: Filter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
    pub anisotropy: bool,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MipType {
    /// Mip maps will be autogenerated from the image data.
    Generate,
    /// Data contains only the highest level mip (lowest detail level). Other mip levels will be
    /// provided later. Contained must be the dimensions of the image.
    Upload(u32, u32),
}

#[derive(Debug, Error)]
#[error("this error should never occur")]
pub struct TextureDataConvertError;

pub trait TextureSource {
    type Error: Error;

    /// Converts the texture source into a raw buffer for uploading to the GPU.
    fn into_texture_data(self) -> Result<TextureData, Self::Error>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextureData {
    data: Vec<u8>,
    width: u32,
    height: u32,
    format: Format,
}

impl TextureHeader {
    pub fn mip_path(root: impl Into<AssetNameBuf>, mip: u32) -> AssetNameBuf {
        let mut path: AssetNameBuf = root.into();
        path.push(mip.to_string());
        path
    }

    pub fn header_path(root: impl Into<AssetNameBuf>) -> AssetNameBuf {
        let mut path: AssetNameBuf = root.into();
        path.push("header.ard_tex");
        path
    }
}

impl TextureSource for TextureData {
    type Error = TextureDataConvertError;

    fn into_texture_data(self) -> Result<TextureData, Self::Error> {
        Ok(self)
    }
}

impl TextureData {
    pub fn new(data: impl Into<Vec<u8>>, width: u32, height: u32, format: Format) -> Self {
        Self {
            data: data.into(),
            width,
            height,
            format,
        }
    }

    #[inline(always)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline(always)]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline(always)]
    pub fn format(&self) -> Format {
        self.format
    }

    #[inline(always)]
    pub fn raw(&self) -> &[u8] {
        &self.data
    }
}
