use ard_assets::prelude::*;
use ard_formats::material::{BlendType, MaterialHeader, MaterialType};
use ard_render::factory::Factory;
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_pbr::{
    PbrMaterialData, PBR_MATERIAL_DIFFUSE_SLOT, PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT,
    PBR_MATERIAL_NORMAL_SLOT,
};
use async_trait::async_trait;

use crate::texture::TextureAsset;

pub struct MaterialLoader {
    factory: Factory,
}

pub struct MaterialAsset {
    pub instance: MaterialInstance,
    pub render_mode: RenderingMode,
}

impl Asset for MaterialAsset {
    const EXTENSION: &'static str = "ard_mat";
    type Loader = MaterialLoader;
}

impl MaterialLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
    }
}

#[async_trait]
impl AssetLoader for MaterialLoader {
    type Asset = MaterialAsset;

    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the material header
        let header = package.read(asset.to_owned()).await?;
        let header = match bincode::deserialize::<MaterialHeader<AssetNameBuf>>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Create material
        let instance = match header.ty {
            MaterialType::Pbr {
                base_color,
                metallic,
                roughness,
                alpha_cutoff,
                diffuse_map,
                normal_map,
                metallic_roughness_map,
            } => {
                let instance = match self.factory.create_pbr_material_instance() {
                    Ok(instance) => instance,
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                };

                // Apply material properties
                self.factory.set_material_data(
                    &instance,
                    &PbrMaterialData {
                        alpha_cutoff,
                        color: base_color,
                        metallic,
                        roughness,
                    },
                );

                // Apply material textures
                if let Some(tex) = diffuse_map {
                    let tex_handle = match assets.load_async(&tex).await {
                        Some(handle) => handle,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };
                    let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                        Some(asset) => asset,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };

                    self.factory.set_material_texture_slot(
                        &instance,
                        PBR_MATERIAL_DIFFUSE_SLOT,
                        Some(&tex_asset.texture),
                    );
                }

                if let Some(tex) = normal_map {
                    let tex_handle = match assets.load_async(&tex).await {
                        Some(handle) => handle,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };
                    let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                        Some(asset) => asset,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };

                    self.factory.set_material_texture_slot(
                        &instance,
                        PBR_MATERIAL_NORMAL_SLOT,
                        Some(&tex_asset.texture),
                    );
                }

                if let Some(tex) = metallic_roughness_map {
                    let tex_handle = match assets.load_async(&tex).await {
                        Some(handle) => handle,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };
                    let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                        Some(asset) => asset,
                        None => {
                            return Err(AssetLoadError::Other("could not get texture asset".into()))
                        }
                    };

                    self.factory.set_material_texture_slot(
                        &instance,
                        PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT,
                        Some(&tex_asset.texture),
                    );
                }

                instance
            }
        };

        Ok(AssetLoadResult::Loaded {
            asset: MaterialAsset {
                instance,
                render_mode: match header.blend_ty {
                    BlendType::Opaque => RenderingMode::Opaque,
                    BlendType::Mask => RenderingMode::AlphaCutout,
                    BlendType::Blend => RenderingMode::Transparent,
                },
            },
            persistent: false,
        })
    }

    async fn post_load(
        &self,
        _assets: Assets,
        _package: Package,
        _handle: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        Ok(AssetPostLoadResult::Loaded)
    }
}
