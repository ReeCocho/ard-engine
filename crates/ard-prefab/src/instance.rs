use ard_assets::prelude::*;
use ard_save_load::entity_map::{EntityMap, MappedEntity};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PrefabInstance {
    asset: AssetNameBuf,
    /// Maps each entity external to the prefab to the mapped entity in the scene asset.
    self_to_parent: Vec<Option<MappedEntity>>,
    /// Maps a set of entities external to the scene to the entities
    /// within the prefab they belong to.
    parent_to_self: Vec<(MappedEntity, MappedEntity)>,
}

impl PrefabInstance {
    pub fn new(
        asset: &AssetName,
        prefab_entity_map: &EntityMap,
        prefab_entity_count: usize,
        parent_entity_map: &EntityMap,
    ) -> Self {
        let self_to_parent = prefab_entity_map
            .mapped()
            .iter()
            .skip(prefab_entity_count)
            .map(|ext_to_prefab| {
                let parent_map = match parent_entity_map.to_map_maybe(*ext_to_prefab) {
                    Some(ent) => ent,
                    None => return None,
                };
                Some(parent_map)
            })
            .collect::<Vec<_>>();

        let parent_to_self = prefab_entity_map
            .mapped()
            .iter()
            .enumerate()
            .take(prefab_entity_count)
            .filter_map(|(i, int_to_prefab)| {
                let parent_map = match parent_entity_map.to_map_maybe(*int_to_prefab) {
                    Some(ent) => ent,
                    None => return None,
                };

                Some((parent_map, MappedEntity(i as u32)))
            })
            .collect::<Vec<_>>();

        Self {
            asset: asset.into(),
            self_to_parent,
            parent_to_self,
        }
    }

    #[inline(always)]
    pub fn asset(&self) -> &AssetName {
        &self.asset
    }
}
