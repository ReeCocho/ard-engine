use ard_assets::prelude::*;
use ard_formats::texture::{MipType, Sampler, TextureData, TextureHeader};
use ard_render::factory::Factory;
use ard_render_textures::texture::{Texture, TextureCreateInfo};
use async_trait::async_trait;

pub struct TextureLoader {
    factory: Factory,
}

pub struct TextureAsset {
    pub texture: Texture,
    post_load: Option<TextureHeader>,
}

impl Asset for TextureAsset {
    const EXTENSION: &'static str = "ard_tex";
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
        // Read in the texture header
        let header = package.read(asset.to_owned()).await?;
        let mut header = match bincode::deserialize::<TextureHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Create the texture. Initialize it with the lowest detail mip
        let path = header.mips.last().unwrap().clone();
        let data = package.read(path).await?;

        // Decode texture data
        let source = match bincode::deserialize::<TextureData>(&data) {
            Ok(data) => data,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Create texture
        let create_info = TextureCreateInfo {
            source,
            debug_name: Some(format!("{asset:?}")),
            mip_count: header.mips.len(),
            mip_type: MipType::Upload(header.width, header.height),
            sampler: Sampler {
                min_filter: header.sampler.min_filter,
                mag_filter: header.sampler.mag_filter,
                mipmap_filter: header.sampler.mipmap_filter,
                address_u: header.sampler.address_u,
                address_v: header.sampler.address_v,
                anisotropy: header.sampler.anisotropy,
            },
        };

        let texture = match self.factory.create_texture(create_info) {
            Ok(texture) => texture,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Pop last mip since it was just loaded
        header.mips.pop();

        // If we have more mips, we need post load
        if header.mips.is_empty() {
            Ok(AssetLoadResult::Loaded {
                asset: TextureAsset {
                    texture,
                    post_load: None,
                },
                persistent: false,
            })
        } else {
            Ok(AssetLoadResult::NeedsPostLoad {
                asset: TextureAsset {
                    texture,
                    post_load: Some(header),
                },
                persistent: false,
            })
        }
    }

    async fn post_load(
        &self,
        assets: Assets,
        package: Package,
        handle: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        let header = assets.get_mut(&handle).unwrap().post_load.take().unwrap();

        for (mip_level, mip_path) in header.mips.into_iter().enumerate().rev() {
            // Load mip
            let data = match package.read(mip_path).await {
                Ok(data) => data,
                Err(err) => return Err(AssetLoadError::Other(err.to_string())),
            };

            // Parse bincode
            let data = bincode::deserialize::<TextureData>(&data).unwrap();

            // Update mip
            self.factory.load_texture_mip(
                &assets.get(&handle).unwrap().texture,
                mip_level as usize,
                data,
            );
        }

        Ok(AssetPostLoadResult::Loaded)
    }
}
