use std::path::PathBuf;
use thiserror::Error;

use ard_engine::{
    assets::prelude::*,
    graphics::{prelude::Factory, TextureFormat},
    graphics_assets::prelude::{PbrMaterialAsset, PbrMaterialDescriptor, TextureAsset},
    log::warn,
    math::Vec4,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::par_task::ParTask;

#[derive(Serialize, Deserialize)]
pub enum AssetMeta {
    Texture {
        width: u32,
        height: u32,
        format: TextureFormat,
    },
    CubeMap {
        width: u32,
        height: u32,
        format: TextureFormat,
    },
    Shader,
    Pipeline,
    Model,
    PbrMaterial {
        asset: AssetNameBuf,
        #[serde(skip)]
        task: ParTask<Handle<PbrMaterialAsset>, AssetMetaError>,
        base_color_texture: Option<AssetNameBuf>,
        roughness_metallic_texture: Option<AssetNameBuf>,
    },
    Unknown,
}

#[derive(Debug, Error, Copy, Clone)]
pub enum AssetMetaError {
    #[error("asset type could not be determined")]
    NoExt,
    #[error("unknown asset meta error")]
    Unknown,
}

pub struct AssetMetaLoader;

impl Asset for AssetMeta {
    const EXTENSION: &'static str = "meta";

    type Loader = AssetMetaLoader;
}

impl AssetMeta {
    /// Draws the inspector editor for an asset. Returns `true` if the asset was modified.
    pub fn draw(&mut self, ui: &imgui::Ui, assets: &Assets, factory: &Factory) -> bool {
        match self {
            AssetMeta::Texture {
                width,
                height,
                format,
            } => {
                ui.text(format!("Width: {}", *width));
                ui.text(format!("Height: {}", *height));
                ui.text(format!("Format: {:?}", *format));
                false
            }
            AssetMeta::PbrMaterial {
                asset,
                task,
                base_color_texture,
                roughness_metallic_texture,
            } => {
                // Update task if needed
                if !task.has_task() {
                    let assets_cl = assets.clone();
                    let asset_cl = asset.clone();
                    *task = ParTask::new(move || {
                        let handle = assets_cl.load::<PbrMaterialAsset>(&asset_cl);
                        assets_cl.wait_for_load(&handle);
                        Ok(handle)
                    });
                }

                let mut modified = false;

                task.ui(ui, |handle| {
                    let mut asset = match assets.get_mut(handle) {
                        Some(asset) => asset,
                        None => {
                            ui.text("There was an error loading the material. Check the logs.");
                            return;
                        }
                    };

                    let mut data = asset.data().clone();

                    // Color
                    let mut base_color_arr =
                        [data.base_color.x, data.base_color.y, data.base_color.z];
                    modified = ui.color_edit3("Base Color", &mut base_color_arr) || modified;
                    data.base_color =
                        Vec4::new(base_color_arr[0], base_color_arr[1], base_color_arr[2], 1.0);

                    // Roughness
                    modified = ui.slider("Roughness", 0.0, 1.0, &mut data.roughness) || modified;

                    // Metallic
                    modified = ui.slider("Metallic", 0.0, 1.0, &mut data.metallic) || modified;

                    // Update the data
                    asset.set_data(factory, data);
                });

                modified
            }
            AssetMeta::Unknown => {
                ui.text("Unknown asset type.");
                false
            }
            _ => false,
        }
    }

    /// Saves the asset meta and associated asset to disk.
    pub fn save(&mut self, assets: &Assets) {
        match self {
            AssetMeta::PbrMaterial {
                asset,
                task,
                base_color_texture,
                roughness_metallic_texture,
            } => {
                let handle = match task.get() {
                    crate::par_task::ParTaskGet::Ok(handle) => handle,
                    _ => {
                        warn!(
                            "attempt to save pbr material `{:?}` failed because it was not loaded",
                            &asset
                        );
                        return;
                    }
                };

                let asset_data = match assets.get(handle) {
                    Some(asset_data) => asset_data,
                    None => {
                        warn!(
                            "attempt to save pbr material `{:?}` failed because it was not loaded",
                            &asset
                        );
                        return;
                    }
                };

                let descriptor = PbrMaterialDescriptor {
                    pipeline: asset_data.pipeline().into(),
                    data: *asset_data.data(),
                };

                let descriptor = match ron::to_string(&descriptor) {
                    Ok(descriptor) => descriptor,
                    Err(err) => {
                        warn!(
                            "attempt to seralize pbr material `{:?}` failed : {:?}",
                            &asset, err
                        );
                        return;
                    }
                };

                let mut path = PathBuf::from("./assets/game/");
                path.push(&asset);

                if let Err(err) = std::fs::write(path, descriptor) {
                    warn!("error saving pbr material `{:?}` : {:?}", asset, err);
                }
            }
            _ => {}
        }
    }

    /// Given an asset name, creates the meta name.
    #[inline]
    pub fn make_meta_name(asset: &AssetName) -> AssetNameBuf {
        let mut name = String::from(asset.file_name().unwrap().to_str().unwrap());
        name.push_str(".meta");

        let mut meta_name = AssetNameBuf::from(asset);
        meta_name.set_file_name(name);

        meta_name
    }

    /// Initiailizes an asset meta file.
    pub fn initialize_for(
        assets: Assets,
        asset: AssetNameBuf,
    ) -> Result<Handle<AssetMeta>, AssetMetaError> {
        let ext = match asset.extension() {
            Some(ext) => String::from(ext.to_str().unwrap()),
            None => return Err(AssetMetaError::NoExt),
        };

        let meta_name = AssetMeta::make_meta_name(&asset);

        let mut path_to_meta = PathBuf::from("./assets/game/");
        path_to_meta.push(&meta_name);

        let contents = match ext.as_str() {
            TextureAsset::EXTENSION => {
                let handle = assets.load::<TextureAsset>(&asset);
                assets.wait_for_load(&handle);

                let asset = assets.get(&handle).unwrap();
                let (width, height) = asset.dims();

                ron::to_string(&AssetMeta::Texture {
                    width,
                    height,
                    format: asset.format(),
                })
                .unwrap()
            }
            PbrMaterialAsset::EXTENSION => {
                let handle = assets.load::<PbrMaterialAsset>(&asset);
                assets.wait_for_load(&handle);

                ron::to_string(&AssetMeta::PbrMaterial {
                    asset: asset.clone(),
                    task: ParTask::default(),
                    base_color_texture: None,
                    roughness_metallic_texture: None,
                })
                .unwrap()
            }
            _ => ron::to_string(&AssetMeta::Unknown).unwrap(),
        };

        // Write the meta data
        std::fs::write(&path_to_meta, contents).unwrap();

        // Register the meta data with the asset manager
        assert!(assets.scan_for(&meta_name));

        // Load the meta file
        let handle = assets.load::<AssetMeta>(&meta_name);
        assets.wait_for_load(&handle);

        Ok(handle)
    }
}

#[async_trait]
impl AssetLoader for AssetMetaLoader {
    type Asset = AssetMeta;

    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;
        let meta = match ron::from_str::<AssetMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        Ok(AssetLoadResult::Loaded {
            asset: meta,
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
