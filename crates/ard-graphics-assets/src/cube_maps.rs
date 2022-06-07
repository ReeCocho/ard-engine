use ard_assets::prelude::*;
use ard_graphics_api::prelude::{FactoryApi, MipType};
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct CubeMapAsset {
    pub cube_map: graphics::CubeMap,
    /// Remaining mips to load
    mips: Vec<CubeFaces>,
    dimensions: (u32, u32),
}

pub struct CubeMapLoader {
    pub(crate) factory: graphics::Factory,
}

#[derive(Debug, Serialize, Deserialize)]
struct CubeMapMeta {
    pub generate_mips: bool,
    pub dimensions: (u32, u32),
    /// Mips should be in order from most detailed to least detailed.
    pub mips: Vec<CubeFaces>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CubeFaces {
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
        let mut meta = match ron::from_str::<CubeMapMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Need at least one mip
        if meta.mips.is_empty() {
            return Err(AssetLoadError::Other(
                String::from("image needs at least one mip").into(),
            ));
        }

        // Load each face
        let mut image_data = Vec::default();
        let dims = meta.dimensions;

        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().east,
            dims,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().west,
            dims,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().top,
            dims,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().bottom,
            dims,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().north,
            dims,
        )
        .await?;
        load_cube_image(
            &mut image_data,
            &package,
            &meta.mips.last().unwrap().south,
            dims,
        )
        .await?;

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
            mip_count: meta.mips.len(),
            mip_type: if meta.generate_mips {
                MipType::Generate
            } else {
                MipType::Upload
            },
        };

        let cube_map = self.factory.create_cube_map(&create_info);

        if meta.generate_mips || meta.mips.len() == 1 {
            Ok(AssetLoadResult::Loaded {
                asset: CubeMapAsset {
                    cube_map,
                    mips: Vec::default(),
                    dimensions: meta.dimensions,
                },
                persistent: false,
            })
        } else {
            // Remove the last mip because it will be uploaded
            meta.mips.pop();

            Ok(AssetLoadResult::NeedsPostLoad {
                asset: CubeMapAsset {
                    cube_map,
                    mips: meta.mips,
                    dimensions: meta.dimensions,
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
    }
}

async fn load_cube_image(
    image_data: &mut Vec<u8>,
    package: &Package,
    file: &AssetName,
    dims: (u32, u32),
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
        Err(err) => return Err(AssetLoadError::Other(err.into())),
    };

    // Make sure dimensions match
    if image.width() > dims.0 || image.height() > dims.1 {
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
