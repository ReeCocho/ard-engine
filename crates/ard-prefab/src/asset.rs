use ard_assets::prelude::*;
use ard_ecs::{prelude::*, system::data::SystemData};
use ard_save_load::{
    format::SaveFormat,
    save_data::{SaveData, Saver},
};
use ard_transform::Children;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PrefabAsset {
    data: SaveData,
}

pub struct PrefabLoader;

impl PrefabAsset {
    pub fn new(data: SaveData) -> Self {
        Self { data }
    }

    pub fn new_from_root<F: SaveFormat>(
        entity: Entity,
        saver: Saver<F>,
        assets: Assets,
        queries: &Queries<Everything>,
    ) -> Result<Self, F::SerializeError> {
        // Collect all children. First child must be the root
        let mut entities = vec![entity];
        fn walk_children(
            parent: Entity,
            queries: &Queries<impl SystemData>,
            out: &mut Vec<Entity>,
        ) {
            let children = match queries.get::<Read<Children>>(parent) {
                Some(children) => children,
                None => return,
            };

            children.0.iter().for_each(|child| {
                out.push(*child);
                walk_children(*child, queries, out);
            });
        }
        walk_children(entity, queries, &mut entities);

        // Mark external entities as null
        let saver = saver.null_external_entities();

        Ok(Self {
            data: saver.save(assets, queries, &entities)?.0,
        })
    }

    #[inline(always)]
    pub fn data(&self) -> &SaveData {
        &self.data
    }
}

impl Asset for PrefabAsset {
    const EXTENSION: &'static str = "ard_pfb";

    type Loader = PrefabLoader;
}

#[async_trait]
impl AssetLoader for PrefabLoader {
    type Asset = PrefabAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let asset_data = package.read(asset.into()).await?;
        let asset = match bincode::deserialize::<PrefabAsset>(&asset_data) {
            Ok(asset) => asset,
            Err(err) => {
                return Err(AssetLoadError::Other(format!(
                    "Could not load prefab `{err:?}`."
                )))
            }
        };

        Ok(AssetLoadResult::Loaded {
            asset,
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
