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
    map_to_entity: HashMap<MappedEntity, Entity>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MappedEntity(pub(crate) usize);

impl Default for MappedEntity {
    #[inline]
    fn default() -> Self {
        Self(usize::MAX)
    }
}

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
        assert!(self
            .entity_to_map
            .insert(entity, MappedEntity(id))
            .is_none());
        self.map_to_entity.insert(MappedEntity(id), entity);
    }

    #[inline]
    pub fn register_from_map(&mut self, mapped: MappedEntity, entity: Entity) {
        assert!(self.map_to_entity.insert(mapped, entity).is_none());
        assert!(self.entity_to_map.insert(entity, mapped).is_none());
    }

    #[inline]
    pub fn to_map(&self, entity: Entity) -> MappedEntity {
        *self.entity_to_map.get(&entity).unwrap()
    }

    #[inline]
    pub fn from_map(&self, mapped: MappedEntity) -> Entity {
        *self.map_to_entity.get(&mapped).unwrap()
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

            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
                pub lighting_settings: crate::lighting::LightingSettingsDescriptor,
                $(
                    // #[serde_as(deserialize_as = "serde_with::DefaultOnError")]
                    // #[serde(default)]
                    [<field_ $field>] : [<$field Pack>],
                )*
            }

            impl [<$name Descriptor>] {
                pub fn new(
                    entities: [<$name Entities>],
                    lighting: &crate::lighting::LightingSettings,
                    queries: &ard_ecs::prelude::Queries<ard_ecs::prelude::Everything>,
                    assets: &ard_assets::manager::Assets,
                ) -> (Self, crate::scene::EntityMap) {
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
                    descriptor.lighting_settings =
                        crate::lighting::LightingSettingsDescriptor::from_settings(
                            lighting,
                            assets
                        );

                    // Save each game object type
                    $(
                        descriptor.[<field_ $field>] = $field::save_to_pack(
                            &entities.[<$field _entities>],
                            queries,
                            &mapping,
                            assets,
                        );
                    )*

                    (descriptor, mapping)
                }

                #[inline]
                pub fn entity_count(&self) -> usize {
                    let mut entity_count = 0;
                    $(
                        entity_count += self.[<field_ $field>].entities.len();
                    )*
                    entity_count
                }

                #[allow(unused_assignments)]
                pub fn load(
                    mut self,
                    commands: &ard_ecs::prelude::EntityCommands,
                    assets: &ard_assets::manager::Assets
                ) -> crate::scene::EntityMap {
                    use crate::object::GameObject;

                    // Create entity mapping
                    let mut entities = [<$name Entities>]::default();
                    let mut mapping = crate::scene::EntityMap::default();
                    $(
                        // Create empty entities
                        entities.[<$field _entities>] =
                            vec![
                                ard_ecs::prelude::Entity::null();
                                self.[<field_ $field>].entities.len()
                            ];
                        commands.create_empty(&mut entities.[<$field _entities>]);

                        // Associate empty entities with the mapped ID
                        for i in 0..self.[<field_ $field>].entities.len() {
                            mapping.register_from_map(
                                self.[<field_ $field>].entities[i],
                                entities.[<$field _entities>][i]
                            );
                        }
                    )*

                    // Load each game object type
                    $(
                        $field::load_from_pack(
                            std::mem::take(&mut self.[<field_ $field>]),
                            &mapping,
                            assets,
                        ).instantiate(&entities.[<$field _entities>], commands);
                    )*

                    mapping
                }
            }
        }
    };
}
