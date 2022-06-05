use ard_assets::prelude::*;
use ard_graphics_api::prelude::FactoryApi;
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct CubeMapAsset {
    pub cube_map: graphics::CubeMap,
}

pub struct CubeMapLoader {
    pub(crate) factory: graphics::Factory,
}

#[derive(Debug, Serialize, Deserialize)]
struct CubeMapMeta {
    pub east: AssetNameBuf,
    pub west: AssetNameBuf,
    pub top: AssetNameBuf,
    pub bottom: AssetNameBuf,
    pub north: AssetNameBuf,
    pub south: AssetNameBuf,
}

impl Asset for CubeMapAsset {
    const EXTENSION: &'static str = "cub";

    type Loader = CubeMapLoader;
}

#[async_trait]
impl AssetLoader for CubeMapLoader {
    type Asset = CubeMapAsset;

    async fn load(
        &self,
        _: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;
        let meta = match ron::from_str::<CubeMapMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Load each face
        let mut image_data = Vec::default();
        let mut dims = (0, 0);

        load_cube_image(&mut image_data, &package, &meta.west, &mut dims).await?;
        load_cube_image(&mut image_data, &package, &meta.east, &mut dims).await?;
        load_cube_image(&mut image_data, &package, &meta.top, &mut dims).await?;
        load_cube_image(&mut image_data, &package, &meta.bottom, &mut dims).await?;
        load_cube_image(&mut image_data, &package, &meta.north, &mut dims).await?;
        load_cube_image(&mut image_data, &package, &meta.south, &mut dims).await?;

        // Create the texture
        let create_info = graphics::CubeMapCreateInfo {
            width: dims.0,
            height: dims.1,
            format: graphics::TextureFormat::R8G8B8A8Srgb,
            data: &image_data,
            sampler: graphics::SamplerDescriptor {
                min_filter: graphics::TextureFilter::Linear,
                max_filter: graphics::TextureFilter::Linear,
                mip_filter: graphics::TextureFilter::Linear,
                x_tiling: graphics::TextureTiling::Repeat,
                y_tiling: graphics::TextureTiling::Repeat,
                anisotropic_filtering: true,
            },
        };

        let cube_map = self.factory.create_cube_map(&create_info);

        Ok(AssetLoadResult::Loaded {
            asset: CubeMapAsset { cube_map },
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

async fn load_cube_image(
    image_data: &mut Vec<u8>,
    package: &Package,
    file: &AssetName,
    dims: &mut (u32, u32),
) -> Result<(), AssetLoadError> {
    let ext = match file.extension() {
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
    let data = package.read(&file).await?;
    let image = match image::load_from_memory_with_format(&data, codec) {
        Ok(image) => image,
        Err(_) => return Err(AssetLoadError::Unknown),
    };

    // Make sure dimensions match
    if dims.0 == 0 || dims.1 == 0 {
        dims.0 = image.width();
        dims.1 = image.height();
    } else if image.width() != dims.0 || image.height() != dims.1 {
        return Err(AssetLoadError::Other(
            String::from("mismatching cube map side dimensions").into(),
        ));
    }

    // Turn into RGBA8 for upload to GPU
    let raw = match image.as_rgba8() {
        Some(raw) => raw,
        None => return Err(AssetLoadError::Unknown),
    };

    // Expand buffer if needed
    if image_data.is_empty() {
        *image_data = Vec::with_capacity(raw.len() * 6);
    }

    image_data.extend_from_slice(&raw);

    Ok(())
}
