use std::time::Duration;

use ard_assets::prelude::*;
use ard_formats::material::{BlendType, MaterialHeader, MaterialType};
use ard_log::warn;
use ard_render::factory::Factory;
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::{MaterialInstance, TextureSlot};
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
    textures: Vec<Option<Handle<TextureAsset>>>,
    pub header: MaterialHeader<AssetNameBuf>,
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

impl MaterialAsset {
    #[inline(always)]
    pub fn render_mode(&self) -> RenderingMode {
        match self.header.blend_ty {
            BlendType::Opaque => RenderingMode::Opaque,
            BlendType::Mask => RenderingMode::AlphaCutout,
            BlendType::Blend => RenderingMode::Transparent,
        }
    }

    #[inline(always)]
    pub fn textures(&self) -> &[Option<Handle<TextureAsset>>] {
        &self.textures
    }

    #[inline(always)]
    pub fn set_texture(
        &mut self,
        factory: &Factory,
        slot: TextureSlot,
        texture: Option<Handle<TextureAsset>>,
    ) {
        const TIMEOUT: Duration = Duration::from_secs(5);

        if let Some(tex) = self.textures.get_mut(usize::from(slot)) {
            match texture {
                Some(texture_handle) => {
                    texture_handle
                        .assets()
                        .wait_for_load_timeout(&texture_handle, TIMEOUT);
                    let handle_cpy = texture_handle.clone();
                    if let Some(texture) = texture_handle.assets().get(&texture_handle) {
                        *tex = Some(handle_cpy);
                        factory.set_material_texture_slot(
                            &self.instance,
                            slot,
                            Some(&texture.texture),
                        );
                    }
                }
                None => {
                    *tex = None;
                    factory.set_material_texture_slot(&self.instance, slot, None);
                }
            }
        }
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

        let mut textures = Vec::default();

        // Create material
        let instance = match header.ty {
            MaterialType::Pbr {
                base_color,
                metallic,
                roughness,
                alpha_cutoff,
                ref diffuse_map,
                ref normal_map,
                ref metallic_roughness_map,
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
                match diffuse_map {
                    Some(tex) => 'tex: {
                        let tex_handle = match assets.load_async(&tex).await {
                            Some(handle) => handle,
                            None => {
                                warn!("Texture `{tex}` does not exist.");
                                textures.push(None);
                                break 'tex;
                            }
                        };
                        let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                            Some(asset) => asset,
                            None => {
                                warn!("Texture `{tex}` could not be loaded.");
                                textures.push(None);
                                break 'tex;
                            }
                        };

                        textures.push(Some(tex_handle.clone()));
                        self.factory.set_material_texture_slot(
                            &instance,
                            PBR_MATERIAL_DIFFUSE_SLOT,
                            Some(&tex_asset.texture),
                        );
                    }
                    None => {}
                }

                match normal_map {
                    Some(tex) => 'tex: {
                        let tex_handle = match assets.load_async(&tex).await {
                            Some(handle) => handle,
                            None => {
                                warn!("Texture `{tex}` does not exist.");
                                textures.push(None);
                                break 'tex;
                            }
                        };
                        let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                            Some(asset) => asset,
                            None => {
                                warn!("Texture `{tex}` could not be loaded.");
                                textures.push(None);
                                break 'tex;
                            }
                        };

                        textures.push(Some(tex_handle.clone()));
                        self.factory.set_material_texture_slot(
                            &instance,
                            PBR_MATERIAL_NORMAL_SLOT,
                            Some(&tex_asset.texture),
                        );
                    }
                    None => {}
                }

                match metallic_roughness_map {
                    Some(tex) => 'tex: {
                        let tex_handle = match assets.load_async(&tex).await {
                            Some(handle) => handle,
                            None => {
                                warn!("Texture `{tex}` does not exist.");
                                textures.push(None);
                                break 'tex;
                            }
                        };
                        let tex_asset = match assets.get::<TextureAsset>(&tex_handle) {
                            Some(asset) => asset,
                            None => {
                                warn!("Texture `{tex}` could not be loaded.");
                                textures.push(None);
                                break 'tex;
                            }
                        };

                        textures.push(Some(tex_handle.clone()));
                        self.factory.set_material_texture_slot(
                            &instance,
                            PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT,
                            Some(&tex_asset.texture),
                        );
                    }
                    None => {}
                }

                instance
            }
        };

        Ok(AssetLoadResult::Loaded {
            asset: MaterialAsset {
                instance,
                textures,
                header,
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
