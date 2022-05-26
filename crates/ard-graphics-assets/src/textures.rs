use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use image::EncodableLayout;
use serde::{Deserialize, Serialize};

/// A texture that can be loaded from disk.
pub struct TextureAsset {
    /// The texture handle.
    pub texture: graphics::Texture,
}

pub struct TextureLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a texture.
#[derive(Debug, Serialize, Deserialize)]
struct TextureMeta {
    /// Path to the actual texture file. Relative to the package.
    pub file: AssetNameBuf,
}

impl Asset for TextureAsset {
    const EXTENSION: &'static str = "tex";

    type Loader = TextureLoader;
}

#[async_trait]
impl AssetLoader for TextureLoader {
    type Asset = TextureAsset;

    async fn load(
        &self,
        _: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;
        let meta = match ron::from_str::<TextureMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        let ext = match meta.file.extension() {
            Some(ext) => match ext.to_str() {
                Some(ext) => ext.to_lowercase(),
                None => return Err(AssetLoadError::Unknown),
            },
            None => return Err(AssetLoadError::Unknown),
        };

        // Determine the texture codec
        let codec = match ext.as_str() {
            "png" => image::ImageFormat::Png,
            "jpeg" | "jpg" => image::ImageFormat::Jpeg,
            "bmp" => image::ImageFormat::Bmp,
            "tga" => image::ImageFormat::Tga,
            "tiff" => image::ImageFormat::Tiff,
            _ => return Err(AssetLoadError::Unknown),
        };

        // Read in the texture data and parse it
        let data = package.read(&meta.file).await?;
        let image = match image::load_from_memory_with_format(&data, codec) {
            Ok(image) => image,
            Err(_) => return Err(AssetLoadError::Unknown),
        };

        // Turn into RGBA8 for upload to GPU
        let raw = match image.as_rgba8() {
            Some(raw) => raw,
            None => return Err(AssetLoadError::Unknown),
        };

        // Create the texture
        let create_info = graphics::TextureCreateInfo {
            width: image.width(),
            height: image.height(),
            format: graphics::TextureFormat::R8G8B8A8Srgb,
            data: raw.as_bytes(),
            mip_type: graphics::MipType::Generate,
            mip_count: 1,
            sampler: graphics::SamplerDescriptor {
                min_filter: graphics::TextureFilter::Linear,
                max_filter: graphics::TextureFilter::Linear,
                mip_filter: graphics::TextureFilter::Linear,
                x_tiling: graphics::TextureTiling::Repeat,
                y_tiling: graphics::TextureTiling::Repeat,
                anisotropic_filtering: true,
            },
        };

        let texture = self.factory.create_texture(&create_info);

        Ok(AssetLoadResult::Loaded {
            asset: TextureAsset { texture },
            persistent: false,
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
