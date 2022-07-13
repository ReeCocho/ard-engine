pub mod static_object;

use ard_assets::manager::Assets;
use ard_ecs::{
    entity::Entity,
    prelude::{EntityCommands, Everything, Queries},
};

pub trait GameObject: Sized {
    type Descriptor;

    /// Creates a default instance of the game object.
    fn create_default(commands: &EntityCommands) -> Entity;

    /// Takes the entities that match the game object and serializes them into a struct of arrays
    /// for fast loading.
    fn save_to_pack(
        entities: &[Entity],
        queries: &Queries<Everything>,
        mapping: &crate::scene::EntityMap,
        assets: &Assets,
    ) -> Self::Descriptor;

    /// After the struct of arrays has been deserialized, this function takes the descriptors and
    /// uses them to construct the components
    fn load_from_pack(
        descriptor: Self::Descriptor,
        entities: &crate::scene::EntityMap,
        assets: &Assets,
    ) -> Self;
}

#[macro_export]
macro_rules! game_object_def {
    ( $name:ident, $( $field:ty )+ ) => {
        paste::paste! {
            #[derive(Default)]
            pub struct $name {
                $(
                    [<field_ $field>]: Vec<$field>,
                )*
            }

            #[serde_with::serde_as]
            #[derive(Default, Serialize, Deserialize)]
            pub struct [<$name Descriptor>] {
                entity_count: usize,
                $(
                    #[serde_as(deserialize_as = "serde_with::DefaultOnError")]
                    #[serde(default)]
                    [<field_ $field>]: Vec<<$field as crate::serialization::SerializableComponent>::Descriptor>,
                )*
            }
        }

        impl crate::object::GameObject for $name {
            paste::paste! {
                type Descriptor = [<$name Descriptor>];
            }

            fn create_default(commands: &ard_ecs::prelude::EntityCommands) -> ard_ecs::prelude::Entity {
                let pack = (
                    $(
                        vec![ <$field>::default() ],
                    )*
                );

                let mut entity = [ard_ecs::prelude::Entity::null()];
                commands.create(pack, &mut entity);
                entity[0]
            }

            fn save_to_pack(
                entities: &[ard_ecs::prelude::Entity],
                queries: &ard_ecs::prelude::Queries<ard_ecs::prelude::Everything>,
                mapping: &crate::scene::EntityMap,
                assets: &ard_assets::manager::Assets
            ) -> Self::Descriptor {
                use crate::serialization::SerializableComponent;

                let mut descriptor = Self::Descriptor::default();
                descriptor.entity_count = entities.len();

                paste::paste!{$(
                    for entity in entities {
                        let comp_descriptor = queries
                            .get::<ard_ecs::prelude::Read<$field>>(*entity)
                            .unwrap()
                            .save(
                                mapping,
                                assets
                            );

                        descriptor.[<field_ $field>].push(comp_descriptor);
                    }
                )*}

                descriptor
            }

            fn load_from_pack(
                mut descriptor: Self::Descriptor,
                entities: &crate::scene::EntityMap,
                assets: &ard_assets::manager::Assets
            ) -> Self {
                let mut components = Self::default();

                // Take the descriptors and convert them into components
                paste::paste! {$(
                    let mem = std::mem::take(&mut descriptor.[<field_ $field>]);
                    components.[<field_ $field>] =
                        <$field as crate::serialization::SerializableComponent>::load(
                            mem,
                            entities,
                            assets
                        ).unwrap();
                )*}

                components
            }
        }
    };
}
