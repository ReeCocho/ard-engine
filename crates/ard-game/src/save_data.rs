use ard_assets::prelude::*;
use ard_core::prelude::*;
use ard_physics::{
    collider::{Collider, ColliderHandle},
    rigid_body::{RigidBody, RigidBodyHandle},
};
use ard_render_assets::loader::{MaterialHandle, MeshHandle};
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_render_objects::{PrevFrameModel, RenderFlags};
use ard_save_load::{
    format::SaveFormat,
    load_data::Loader,
    save_data::{SaveData, Saver},
};
use ard_transform::{Children, Model, Parent, Position, Rotation, Scale, SetParent};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::components::{actor::Actor, player::PlayerSpawn, stat::MarkStatic};

pub const INITIAL_SCENE_ASSET_NAME: &'static str = "initial_scene.ard_isav";

#[derive(Serialize, Deserialize)]
pub struct SceneAssetHeader {
    pub data_path: AssetNameBuf,
}

#[derive(Serialize, Deserialize)]
pub struct InitialSceneAsset {
    pub asset_name: AssetNameBuf,
}

#[derive(Serialize, Deserialize)]
pub struct SceneAsset {
    data: SaveData,
}

pub struct SceneLoader;
pub struct InitialSceneLoader;

impl SceneAsset {
    pub fn saver<F: SaveFormat + 'static>() -> Saver<F> {
        Saver::default()
            .include_component::<Position>()
            .include_component::<Rotation>()
            .include_component::<Scale>()
            .include_component::<Parent>()
            .include_component::<Children>()
            .include_component::<Model>()
            .include_component::<PrevFrameModel>()
            .include_component::<RenderingMode>()
            .include_component::<RenderFlags>()
            .include_component::<MeshHandle>()
            .include_component::<MaterialHandle>()
            .include_component::<Name>()
            .include_component::<MarkStatic>()
            .include_component::<Collider>()
            .include_component::<RigidBody>()
            .include_component::<Actor>()
            .include_component::<PlayerSpawn>()
            .ignore::<ColliderHandle>()
            .ignore::<RigidBodyHandle>()
            .ignore::<Static>()
            .ignore::<Mesh>()
            .ignore::<MaterialInstance>()
            .ignore::<Destroy>()
            .ignore::<SetParent>()
    }

    pub fn loader<F: SaveFormat + 'static>() -> Loader<F> {
        Loader::default()
            .load_component::<Position>()
            .load_component::<Rotation>()
            .load_component::<Scale>()
            .load_component::<Parent>()
            .load_component::<Children>()
            .load_component::<Model>()
            .load_component::<PrevFrameModel>()
            .load_component::<RenderingMode>()
            .load_component::<RenderFlags>()
            .load_component::<MeshHandle>()
            .load_component::<MaterialHandle>()
            .load_component::<Name>()
            .load_component::<MarkStatic>()
            .load_component::<Collider>()
            .load_component::<RigidBody>()
            .load_component::<Actor>()
            .load_component::<PlayerSpawn>()
    }

    #[inline(always)]
    pub fn data(&self) -> &SaveData {
        &self.data
    }
}

impl Asset for SceneAsset {
    const EXTENSION: &'static str = "ard_sav";
    type Loader = SceneLoader;
}

impl Asset for InitialSceneAsset {
    const EXTENSION: &'static str = "ard_isav";
    type Loader = InitialSceneLoader;
}

#[async_trait]
impl AssetLoader for SceneLoader {
    type Asset = SceneAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let header = package.read(asset.to_owned()).await?;
        let header = match bincode::deserialize::<SceneAssetHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        let data = package.read(header.data_path.clone()).await?;
        let asset = match bincode::deserialize(&data) {
            Ok(asset) => asset,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        Ok(AssetLoadResult::Loaded {
            asset,
            persistent: true,
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

#[async_trait]
impl AssetLoader for InitialSceneLoader {
    type Asset = InitialSceneAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let asset = package.read(asset.to_owned()).await?;
        let asset = match bincode::deserialize::<InitialSceneAsset>(&asset) {
            Ok(asset) => asset,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        Ok(AssetLoadResult::Loaded {
            asset,
            persistent: true,
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
