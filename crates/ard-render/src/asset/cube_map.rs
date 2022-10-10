use ard_assets::prelude::*;
use ard_pal::prelude::{Filter, SamplerAddressMode, TextureFormat};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    cube_map::{CubeMap, CubeMapCreateInfo},
    factory::Factory,
    texture::{MipType, Sampler},
};

pub struct CubeMapAsset {
    pub cube_map: CubeMap,
    /// Remaining mips to load
    mips: Vec<CubeFaces>,
    size: u32,
}

pub struct CubeMapLoader {
    factory: Factory,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CubeMapDescriptor {
    pub generate_mips: bool,
    pub size: u32,
    /// Mips should be in order from most detailed to least detailed.
    pub mips: Vec<CubeFaces>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CubeFaces {
    pub east: AssetNameBuf,
    pub west: AssetNameBuf,
    pub top: AssetNameBuf,
    pub bottom: AssetNameBuf,
    pub north: AssetNameBuf,
    pub south: AssetNameBuf,
}

impl Asset for CubeMapAsset {
    const EXTENSION: &'static str = "cube";
    type Loader = CubeMapLoader;
}

impl CubeMapLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
    }
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
        let meta = match ron::from_str::<CubeMapDescriptor>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Need at least one mip
        if meta.mips.is_empty() {
            return Err(AssetLoadError::Other(
                String::from("image needs at least one mip").into(),
            ));
        }

        // Load each face
        let mut image_data = Vec::default();
        let size = meta.size;

        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().east,
            size,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().west,
            size,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().top,
            size,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().bottom,
            size,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().north,
            size,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().south,
            size,
        )
        .await?;

        // Create the texture
        let create_info = CubeMapCreateInfo {
            size,
            format: TextureFormat::Rgba8Srgb,
            data: &image_data,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
                address_u: SamplerAddressMode::ClampToEdge,
                address_v: SamplerAddressMode::ClampToEdge,
                anisotropy: false,
            },
            mip_count: meta.mips.len(),
            mip_type: if meta.generate_mips {
                MipType::Generate
            } else {
                MipType::Upload
            },
        };

        let cube_map = self.factory.create_cube_map(create_info);

        if meta.generate_mips || meta.mips.len() == 1 {
            Ok(AssetLoadResult::Loaded {
                asset: CubeMapAsset {
                    cube_map,
                    mips: Vec::default(),
                    size: meta.size,
                },
                persistent: false,
            })
        } else {
            todo!();

            // Remove the last mip because it will be uploaded
            meta.mips.pop();

            Ok(AssetLoadResult::NeedsPostLoad {
                asset: CubeMapAsset {
                    cube_map,
                    mips: meta.mips,
                    size: meta.size,
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
        todo!()
        /*
        // Get the next set of faces to load
        let (faces, level, dims) = {
            let mut asset = assets.get_mut(&handle).unwrap();
            let faces = asset.mips.pop();
            (faces, asset.mips.len(), asset.dimensions)
        };

        let faces = match faces {
            Some(faces) => faces,
            None => return Ok(AssetPostLoadResult::Loaded),
        };

        // Load each face
        let mut image_data = Vec::default();
        load_cube_image(&mut image_data, &package, &faces.east, dims).await?;
        load_cube_image(&mut image_data, &package, &faces.west, dims).await?;
        load_cube_image(&mut image_data, &package, &faces.top, dims).await?;
        load_cube_image(&mut image_data, &package, &faces.bottom, dims).await?;
        load_cube_image(&mut image_data, &package, &faces.north, dims).await?;
        load_cube_image(&mut image_data, &package, &faces.south, dims).await?;

        // Upload the mip
        self.factory
            .load_cube_map_mip(&assets.get(&handle).unwrap().cube_map, level, &image_data);

        Ok(AssetPostLoadResult::NeedsPostLoad)
        */
    }
}

async fn load_cube_image(
    image_data: &mut Vec<u8>,
    package: &Package,
    file: &AssetName,
    size: u32,
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
        Err(err) => return Err(AssetLoadError::Other(err.to_string())),
    };

    // Make sure dimensions match
    if image.width() > size || image.height() > size {
        return Err(AssetLoadError::Other(
            String::from("mismatching cube map side dimensions").into(),
        ));
    }

    // Turn into RGBA8 for upload to GPU
    let raw = image.to_rgba8();

    // Expand buffer if needed
    if image_data.is_empty() {
        *image_data = Vec::with_capacity(raw.len() * 6);
    }

    image_data.extend_from_slice(&raw);

    Ok(())
}
