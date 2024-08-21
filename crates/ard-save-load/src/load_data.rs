use std::collections::HashMap;

use ard_assets::manager::Assets;
use ard_ecs::{
    archetype::{storage::AnyArchetypeStorage, Archetype, ArchetypeId, Archetypes},
    key::TypeKey,
    prelude::*,
    tag::{pack::TagPack, TagCollection, TagCollectionId},
};
use rustc_hash::FxHashMap;

use crate::{
    entity_map::EntityMap,
    format::SaveFormat,
    loader::{ComponentLoader, GenericComponentLoader, GenericTagLoader, TagLoader},
    save_data::SaveData,
    LoadContext, SaveLoad,
};

pub struct Loader<F: SaveFormat> {
    meta_data: FxHashMap<String, LoadingMetaData<F>>,
}

impl<F: SaveFormat> Default for Loader<F> {
    fn default() -> Self {
        Self {
            meta_data: FxHashMap::default(),
        }
    }
}

impl<F: SaveFormat + 'static> Loader<F> {
    pub fn load_component<C: Component + SaveLoad + 'static>(mut self) -> Self {
        self.meta_data.insert(
            C::NAME.into(),
            LoadingMetaData::Component {
                new_loader: |ctx, raw| {
                    let mut loader = ComponentLoader::<F, C>::default();
                    loader.deserialize_all(ctx, raw)?;
                    Ok(Box::new(loader))
                },
            },
        );
        self
    }

    pub fn load_tag<T: Tag + SaveLoad + 'static>(mut self) -> Self {
        self.meta_data.insert(
            T::NAME.into(),
            LoadingMetaData::Tag {
                new_loader: |ctx, raw| {
                    let mut loader = TagLoader::<F, T>::default();
                    loader.deserialize_all(ctx, raw)?;
                    Ok(Box::new(loader))
                },
            },
        );
        self
    }

    pub fn load(
        self,
        data: SaveData,
        assets: Assets,
        commands: &EntityCommands,
    ) -> Result<(), F::DeserializeError> {
        self.load_with_external(data, assets, commands, None, &[])
    }

    pub fn load_with_external(
        self,
        data: SaveData,
        assets: Assets,
        commands: &EntityCommands,
        load_into: Option<&[Entity]>,
        external: &[Entity],
    ) -> Result<(), F::DeserializeError> {
        let entities = match load_into {
            Some(entities) => {
                assert_eq!(entities.len(), data.entity_count);
                Vec::from_iter(entities.iter().cloned())
            }
            None => {
                let mut entities = Vec::with_capacity(data.entity_count);
                entities.resize(data.entity_count, Entity::null());
                commands.create_empty(&mut entities);
                entities
            }
        };

        let mut entity_map = EntityMap::new_from_entities(&entities);
        external.iter().for_each(|e| {
            entity_map.to_map(*e);
        });

        let mut ctx = LoadContext { entity_map, assets };

        for archetype in data.archetypes {
            let remapped_entities: Vec<_> = archetype
                .entities
                .into_iter()
                .map(|e| ctx.entity_map.from_map(e))
                .collect();

            let loaders = archetype
                .buffers
                .into_iter()
                .map(|buffer| {
                    let meta = self.meta_data.get(&buffer.type_name).unwrap();
                    match meta {
                        LoadingMetaData::Component { new_loader, .. } => {
                            new_loader(&mut ctx, buffer.raw)
                        }
                        _ => unreachable!(),
                    }
                })
                .collect::<Result<Vec<_>, _>>();

            let loaders = match loaders {
                Ok(loaders) => loaders,
                Err(err) => {
                    if load_into.is_none() {
                        commands.destroy(&entities);
                    }
                    return Err(err);
                }
            };

            commands.set_components(
                &remapped_entities,
                LoadedComponentPack {
                    entity_count: remapped_entities.len(),
                    loaders,
                },
            );
        }

        for collection in data.collections {
            let remapped_entities: Vec<_> = collection
                .entities
                .into_iter()
                .map(|e| ctx.entity_map.from_map(e))
                .collect();

            let loaders = collection
                .buffers
                .into_iter()
                .map(|buffer| {
                    let meta = self.meta_data.get(&buffer.type_name).unwrap();
                    match meta {
                        LoadingMetaData::Tag { new_loader, .. } => new_loader(&mut ctx, buffer.raw),
                        _ => unreachable!(),
                    }
                })
                .collect::<Result<Vec<_>, _>>();

            let loaders = match loaders {
                Ok(loaders) => loaders,
                Err(err) => {
                    if load_into.is_none() {
                        commands.destroy(&entities);
                    }
                    return Err(err);
                }
            };

            commands.set_tags(
                &remapped_entities,
                LoadedTagPack {
                    entity_count: remapped_entities.len(),
                    loaders,
                },
            );
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
enum LoadingMetaData<F: SaveFormat> {
    Component {
        new_loader: fn(
            &mut LoadContext,
            Vec<u8>,
        ) -> Result<Box<dyn GenericComponentLoader<F>>, F::DeserializeError>,
    },
    Tag {
        new_loader: fn(
            &mut LoadContext,
            Vec<u8>,
        ) -> Result<Box<dyn GenericTagLoader<F>>, F::DeserializeError>,
    },
}

struct LoadedComponentPack<F: SaveFormat> {
    entity_count: usize,
    loaders: Vec<Box<dyn GenericComponentLoader<F>>>,
}

struct LoadedTagPack<F: SaveFormat> {
    entity_count: usize,
    loaders: Vec<Box<dyn GenericTagLoader<F>>>,
}

impl<F: SaveFormat> ComponentPack for LoadedComponentPack<F> {
    fn is_valid(&self) -> bool {
        true
    }

    fn len(&self) -> usize {
        self.entity_count
    }

    fn is_empty(&self) -> bool {
        self.entity_count == 0
    }

    fn type_key(&self) -> TypeKey {
        let mut type_key = TypeKey::default();
        self.loaders.iter().for_each(|loader| {
            type_key.add_by_id(loader.type_id());
        });
        type_key
    }

    fn move_into(self, entities: &[Entity], archetypes: &mut Archetypes) -> (ArchetypeId, usize) {
        let type_key = ComponentPack::type_key(&self);

        let (archetype, index) = if let Some(archetype) = archetypes.get_archetype(&type_key) {
            archetype
        } else {
            let mut archetype = Archetype {
                type_key: type_key.clone(),
                map: HashMap::default(),
                entities: 0,
            };

            self.loaders.iter().for_each(|loader| {
                let type_id = loader.type_id();
                let storage_id = loader.new_storage(archetypes);
                archetype.map.insert(type_id, storage_id);
            });

            archetype.entities = archetypes.get_entity_storage_mut().create();
            archetypes.add_archetype(archetype);
            archetypes.get_archetype(&type_key).unwrap()
        };
        let archetype: &Archetype = archetype;

        let begin_ind = {
            let mut entity_buffer = archetypes.get_entity_storage().get_mut(archetype.entities);
            let begin = entity_buffer.len();
            entity_buffer.extend_from_slice(entities);
            begin
        };

        self.loaders.into_iter().for_each(|mut loader| {
            let type_id = loader.type_id();
            let ind = *archetype.map.get(&type_id).unwrap();
            loader.move_into(archetypes, ind);
        });

        (index, begin_ind)
    }
}

impl<F: SaveFormat> TagPack for LoadedTagPack<F> {
    fn is_valid(&self) -> bool {
        true
    }

    fn len(&self) -> usize {
        self.entity_count
    }

    fn is_empty(&self) -> bool {
        self.entity_count == 0
    }

    fn move_into(&mut self, entities: &[Entity], tags: &mut Tags) -> TagCollectionId {
        let type_key = TagPack::type_key(self);

        std::mem::take(&mut self.loaders)
            .into_iter()
            .for_each(|mut loader| {
                loader.move_into(tags, entities);
            });

        match tags.get_collection_id(&type_key) {
            Some(id) => id,
            None => tags.add_collection(TagCollection { type_key }),
        }
    }

    fn type_key(&self) -> TypeKey {
        let mut type_key = TypeKey::default();
        self.loaders.iter().for_each(|loader| {
            type_key.add_by_id(loader.type_id());
        });
        type_key
    }
}
