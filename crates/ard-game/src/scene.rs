use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use ard_assets::prelude::AssetNameBuf;
use ard_ecs::entity::Entity;

pub struct SceneDescriptor {
    pub textures: Vec<AssetNameBuf>,
}

pub struct Scene {}

#[derive(Default)]
pub struct EntityMap {
    entity_to_map: HashMap<Entity, MappedEntity>,
    map_to_entity: Vec<Entity>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MappedEntity(usize);

impl EntityMap {
    #[inline]
    pub fn len(&self) -> usize {
        self.map_to_entity.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn register(&mut self, entity: Entity) {
        let id = self.map_to_entity.len();
        self.map_to_entity.push(entity);
        assert!(self
            .entity_to_map
            .insert(entity, MappedEntity(id))
            .is_none());
    }

    #[inline]
    pub fn to_map(&self, entity: Entity) -> MappedEntity {
        *self.entity_to_map.get(&entity).unwrap()
    }

    #[inline]
    pub fn from_map(&self, mapped: MappedEntity) -> Entity {
        self.map_to_entity[mapped.0]
    }
}

#[macro_export]
macro_rules! scene_definition {
    ( $name:ident, $( $field:ty )+ ) => {
        paste::paste! {
            #[derive(Default)]
            pub struct $name {
                $(
                    [<field_ $field>] : $field,
                )*
            }

            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub enum [<$name GameObject>] {
                $(
                    $field,
                )*
            }

            #[derive(Default)]
            pub struct [<$name Entities>] {
                $(
                    pub [<$field _entities>]: Vec<ard_ecs::prelude::Entity>,
                )*
            }

            #[serde_with::serde_as]
            #[derive(Default, Serialize, Deserialize)]
            pub struct [<$name Descriptor>] {
                $(
                    #[serde_as(deserialize_as = "serde_with::DefaultOnError")]
                    #[serde(default)]
                    [<field_ $field>] : [<$field Descriptor>],
                )*
            }

            impl [<$name Descriptor>] {
                pub fn new(
                    entities: [<$name Entities>],
                    queries: &ard_ecs::prelude::Queries<ard_ecs::prelude::Everything>,
                    assets: &ard_assets::manager::Assets,
                ) -> Self {
                    use crate::object::GameObject;

                    // Create the entity map
                    let mut mapping = crate::scene::EntityMap::default();

                    $(
                        for entity in &entities.[<$field _entities>] {
                            mapping.register(*entity);
                        }
                    )*

                    // Save game objects
                    let mut descriptor = Self::default();

                    // Save each game object type
                    $(
                        descriptor.[<field_ $field>] = $field::save_to_pack(
                            &entities.[<$field _entities>],
                            queries,
                            &mapping,
                            assets,
                        );
                    )*

                    descriptor
                }
            }
        }
    };
}
