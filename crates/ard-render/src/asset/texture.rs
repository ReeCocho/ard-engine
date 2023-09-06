use ard_assets::prelude::*;
use ard_pal::prelude::{Filter, Format, SamplerAddressMode};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    prelude::Factory,
    texture::{MipType, Sampler, Texture, TextureCreateInfo},
};

pub struct TextureAsset {
    pub texture: Texture,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextureDescriptor {
    pub path: AssetNameBuf,
}

pub struct TextureLoader {
    factory: Factory,
}

impl Asset for TextureAsset {
    const EXTENSION: &'static str = "tex";
    type Loader = TextureLoader;
}

impl TextureLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
    }
}

#[async_trait]
impl AssetLoader for TextureLoader {
    type Asset = TextureAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let desc = package.read_str(asset).await?;
        let desc = match ron::from_str::<TextureDescriptor>(&desc) {
            Ok(desc) => desc,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        let (bytes, width, height, format) = match desc.path.extension() {
            Some(_) => {
                let image_data = package.read(&desc.path).await?;
                let image = match image::load_from_memory(&image_data) {
                    Ok(image) => image,
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                };

                let raw = match image.as_rgba8() {
                    Some(image) => image,
                    None => {
                        return Err(AssetLoadError::Other(
                            "Could not convert image to RGBA8.".to_owned(),
                        ))
                    }
                };

                (
                    raw.to_vec(),
                    image.width(),
                    image.height(),
                    Format::Rgba8Unorm,
                )
            }
            None => todo!(),
        };

        let texture = self.factory.create_texture(TextureCreateInfo {
            width,
            height,
            format,
            data: &bytes,
            mip_type: MipType::Upload,
            mip_count: 1,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
                address_u: SamplerAddressMode::Repeat,
                address_v: SamplerAddressMode::Repeat,
                anisotropy: false,
            },
        });

        Ok(AssetLoadResult::Loaded {
            asset: TextureAsset { texture },
            persistent: true,
        })
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}
