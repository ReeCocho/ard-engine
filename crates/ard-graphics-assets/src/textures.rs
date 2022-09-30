use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A texture that can be loaded from disk.
pub struct TextureAsset {
    /// The texture handle.
    pub texture: graphics::Texture,
    /// Texture dimensions
    dims: (u32, u32),
    /// Format used by the texture.
    format: graphics::TextureFormat,
}

pub struct TextureLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a texture.
#[derive(Debug, Serialize, Deserialize)]
pub struct TextureDescriptor {
    /// Path to the actual texture file. Relative to the package.
    pub file: AssetNameBuf,
    pub format: TextureFormat,
}

impl TextureAsset {
    #[inline]
    pub fn dims(&self) -> (u32, u32) {
        self.dims
    }

    #[inline]
    pub fn format(&self) -> graphics::TextureFormat {
        self.format
    }
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
        let meta = match ron::from_str::<TextureDescriptor>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        let ext = match meta.file.extension() {
            Some(ext) => match ext.to_str() {
                Some(ext) => ext.to_lowercase(),
                None => {
                    return Err(AssetLoadError::Other(
                        String::from("unable to get file extension for texture").into(),
                    ))
                }
            },
            None => {
                return Err(AssetLoadError::Other(
                    String::from("texture has no file extension").into(),
                ))
            }
        };

        // Determine the texture codec
        let codec = match ext.as_str() {
            "png" => image::ImageFormat::Png,
            "jpeg" | "jpg" => image::ImageFormat::Jpeg,
            "bmp" => image::ImageFormat::Bmp,
            "tga" => image::ImageFormat::Tga,
            "tiff" => image::ImageFormat::Tiff,
            "hdr" => image::ImageFormat::Hdr,
            _ => {
                return Err(AssetLoadError::Other(
                    String::from("unsupported texture codec").into(),
                ))
            }
        };

        // Read in the texture data and parse it
        let data = package.read(&meta.file).await?;
        let image = match image::load_from_memory_with_format(&data, codec) {
            Ok(image) => image,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Turn into RGBA8 for upload to GPU
        let raw = match meta.format {
            TextureFormat::R8G8B8A8Unorm | TextureFormat::R8G8B8A8Srgb => {
                image::DynamicImage::from(image.to_rgba8())
            }
            TextureFormat::R16G16B16A16Unorm => image::DynamicImage::from(image.to_rgba16()),
            TextureFormat::R32G32B32A32Sfloat => image::DynamicImage::from(image.to_rgba32f()),
            _ => todo!(),
        };

        // let (raw, format, converted) = match &image {
        //     image::DynamicImage::ImageRgb8(_) => {
        //         let converted = Some(image.to_rgba8());
        //         (converted.as_ref().unwrap().as_bytes(), graphics::TextureFormat::R8G8B8A8Unorm, converted)
        //     },
        //     image::DynamicImage::ImageRgba8(img) => (img.as_bytes(), graphics::TextureFormat::R8G8B8A8Srgb, None),
        //     image::DynamicImage::ImageRgba16(img) => (img.as_bytes(), graphics::TextureFormat::R16G16B16A16Unorm, None),
        //     image::DynamicImage::ImageRgba32F(img) => (img.as_bytes(), graphics::TextureFormat::R32G32B32A32Sfloat, None),
        //     _ => return Err(AssetLoadError::Other(String::from("unsupported image type").into())),
        // };

        // Create the texture
        let create_info = graphics::TextureCreateInfo {
            width: image.width(),
            height: image.height(),
            format: meta.format,
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
            asset: TextureAsset {
                texture,
                format: meta.format,
                dims: (image.width(), image.height()),
            },
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
