use std::path::Path;

use crate::{
    cube_map::CubeMapCreateInfo,
    texture::{MipType, Sampler},
};
use ard_assets::prelude::*;
use ard_formats::cube_map::CubeMapHeader;

use super::{CubeMapAsset, CubeMapLoader, CubeMapPostLoad};

pub(crate) async fn to_asset(
    path: &Path,
    package: &Package,
    loader: &CubeMapLoader,
) -> Result<CubeMapAsset, AssetLoadError> {
    // Read in the header
    let mut header_path = path.to_path_buf();
    header_path.push("header");
    let header = match bincode::deserialize::<CubeMapHeader>(&package.read(&header_path).await?) {
        Ok(header) => header,
        Err(err) => return Err(AssetLoadError::Other(err.to_string())),
    };

    // Read in the lowest detail mip
    let mip_to_load = header.mip_count - 1;
    let mut mip_path = path.to_path_buf();
    mip_path.push(format!("{mip_to_load}"));
    let mip_data = package.read(&mip_path).await?;

    // Create the cube map
    let create_info = CubeMapCreateInfo {
        size: header.size,
        format: header.format,
        data: &mip_data,
        mip_type: MipType::Upload,
        mip_count: header.mip_count as usize,
        sampler: Sampler {
            min_filter: header.sampler.min_filter,
            mag_filter: header.sampler.mag_filter,
            mipmap_filter: header.sampler.mipmap_filter,
            address_u: header.sampler.address_u,
            address_v: header.sampler.address_v,
            anisotropy: false,
        },
    };
    let cube_map = loader.factory.create_cube_map(create_info);

    Ok(CubeMapAsset {
        cube_map,
        post_load: Some(CubeMapPostLoad::Ard {
            path: path.to_path_buf(),
            next_mip: if header.mip_count == 1 {
                None
            } else {
                Some(header.mip_count as usize - 1)
            },
        }),
    })
}
