use std::any::TypeId;

use ard_assets::manager::Assets;
use ard_ecs::{key::TypeKey, prelude::*};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    entity_map::{EntityMap, MappedEntity},
    format::SaveFormat,
    saver::{ComponentSaver, GenericSaver, TagSaver},
    SaveContext, SaveLoad,
};

pub struct Saver<F: SaveFormat> {
    meta_data: FxHashMap<TypeId, SavingMetaData<F>>,
}

impl<F: SaveFormat> Default for Saver<F> {
    fn default() -> Self {
        Self {
            meta_data: FxHashMap::default(),
        }
    }
}

impl<F: SaveFormat + 'static> Saver<F> {
    pub fn ignore<T: 'static>(mut self) -> Self {
        self.meta_data
            .insert(TypeId::of::<T>(), SavingMetaData::Ignore);
        self
    }

    pub fn include_component<T: Component + SaveLoad + 'static>(mut self) -> Self {
        self.meta_data.insert(
            TypeId::of::<T>(),
            SavingMetaData::Info {
                type_name: T::NAME.to_owned(),
                new_saver: || Box::new(ComponentSaver::<F, T>::default()),
            },
        );
        self
    }

    pub fn include_tag<T: Tag + SaveLoad + 'static>(mut self) -> Self {
        self.meta_data.insert(
            TypeId::of::<T>(),
            SavingMetaData::Info {
                type_name: T::NAME.to_owned(),
                new_saver: || Box::new(TagSaver::<F, T>::default()),
            },
        );
        self
    }

    pub fn save(
        mut self,
        assets: Assets,
        queries: &Queries<Everything>,
        entities: &[Entity],
    ) -> Result<(SaveData, EntityMap), F::SerializeError> {
        let mut archetypes = FxHashMap::<TypeKey, SavingData<F>>::default();
        let mut collections = FxHashMap::<TypeKey, SavingData<F>>::default();

        let mut ctx = SaveContext {
            assets,
            entity_map: EntityMap::new_from_entities(entities),
        };

        let entity_count = ctx.entity_map.len();

        entities.iter().for_each(|e| {
            let e = *e;
            self.save_components(&mut archetypes, &mut ctx, queries, e);
            self.save_tags(&mut collections, &mut ctx, queries, e);
        });

        let save_data = SaveData {
            entity_count,
            archetypes: archetypes
                .into_values()
                .map(|a| a.to_saved(&self, &mut ctx.entity_map))
                .collect::<Result<Vec<_>, _>>()?,
            collections: collections
                .into_values()
                .map(|c| c.to_saved(&self, &mut ctx.entity_map))
                .collect::<Result<Vec<_>, _>>()?,
        };

        Ok((save_data, ctx.entity_map))
    }

    fn save_components(
        &mut self,
        archetypes: &mut FxHashMap<TypeKey, SavingData<F>>,
        ctx: &mut SaveContext,
        queries: &Queries<Everything>,
        entity: Entity,
    ) {
        let component_types = queries.component_types(entity);
        let mut final_key = TypeKey::default();
        component_types.iter().for_each(|ty| {
            if let SavingMetaData::Ignore = self.meta_data.get(ty).unwrap() {
                return;
            }
            final_key.add_by_id(*ty);
        });

        let entry = archetypes.entry(final_key.clone()).or_insert_with(|| {
            let mut data = SavingData::default();
            final_key
                .iter()
                .for_each(|ty| match self.meta_data.get(ty).unwrap() {
                    SavingMetaData::Ignore => unreachable!(),
                    SavingMetaData::Info { new_saver, .. } => {
                        data.savers.insert(*ty, new_saver());
                    }
                });
            data
        });
        entry.entities.push(entity);

        final_key.iter().for_each(|ty| {
            entry.savers.get_mut(ty).unwrap().add(ctx, entity, queries);
        });
    }

    fn save_tags(
        &mut self,
        collections: &mut FxHashMap<TypeKey, SavingData<F>>,
        ctx: &mut SaveContext,
        queries: &Queries<Everything>,
        entity: Entity,
    ) {
        let tag_types = queries.tag_types(entity);
        let mut final_key = TypeKey::default();
        tag_types.iter().for_each(|ty| {
            if let SavingMetaData::Ignore = self.meta_data.get(ty).unwrap() {
                return;
            }
            final_key.add_by_id(*ty);
        });

        let entry = collections.entry(final_key.clone()).or_insert_with(|| {
            let mut data = SavingData::<F>::default();
            final_key
                .iter()
                .for_each(|ty| match self.meta_data.get(ty).unwrap() {
                    SavingMetaData::Ignore => unreachable!(),
                    SavingMetaData::Info { new_saver, .. } => {
                        data.savers.insert(*ty, new_saver());
                    }
                });
            data
        });
        entry.entities.push(entity);

        final_key.iter().for_each(|ty| {
            entry.savers.get_mut(ty).unwrap().add(ctx, entity, queries);
        });
    }
}

enum SavingMetaData<F: SaveFormat> {
    Ignore,
    Info {
        type_name: String,
        new_saver: fn() -> Box<dyn GenericSaver<F>>,
    },
}

struct SavingData<F: SaveFormat> {
    pub entities: Vec<Entity>,
    pub savers: FxHashMap<TypeId, Box<dyn GenericSaver<F>>>,
}

impl<F: SaveFormat> Default for SavingData<F> {
    fn default() -> Self {
        Self {
            entities: Vec::default(),
            savers: FxHashMap::default(),
        }
    }
}

impl<F: SaveFormat> SavingData<F> {
    fn to_saved(
        self,
        saver: &Saver<F>,
        map: &mut EntityMap,
    ) -> Result<SavedSet, F::SerializeError> {
        let buffers = self
            .savers
            .into_iter()
            .map(|(ty, mut buff_saver)| {
                let type_name = match saver.meta_data.get(&ty).unwrap() {
                    SavingMetaData::Ignore => unreachable!(),
                    SavingMetaData::Info { type_name, .. } => type_name.clone(),
                };

                Ok(SavedDataBuffer {
                    type_name,
                    raw: buff_saver.serialize_all()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SavedSet {
            entities: self
                .entities
                .into_iter()
                .map(|e| map.to_map_or_insert(e))
                .collect(),
            buffers,
        })
    }
}

impl SaveData {
    pub fn contains_component<C: Component>(&self) -> bool {
        for set in self.archetypes.iter() {
            if set.contains_component::<C>() {
                return true;
            }
        }
        false
    }
}

impl SavedSet {
    pub fn contains_component<C: Component>(&self) -> bool {
        for buffer in self.buffers.iter() {
            if buffer.type_name == C::NAME {
                return true;
            }
        }
        false
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SaveData {
    pub entity_count: usize,
    pub archetypes: Vec<SavedSet>,
    pub collections: Vec<SavedSet>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedSet {
    pub entities: Vec<MappedEntity>,
    pub buffers: Vec<SavedDataBuffer>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedDataBuffer {
    pub type_name: String,
    pub raw: Vec<u8>,
}
