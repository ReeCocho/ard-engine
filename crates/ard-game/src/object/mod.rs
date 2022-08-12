pub mod empty;
pub mod static_object;

use ard_assets::manager::Assets;
use ard_ecs::{
    entity::Entity,
    prelude::{EntityCommands, Everything, Queries},
};

pub trait GameObject: Sized {
    type Pack;

    /// Creates a default instance of the game object.
    fn create_default(commands: &EntityCommands) -> Entity;

    /// Takes the game object set and instantiates it in the world.
    fn instantiate(self, entities: &[Entity], commands: &EntityCommands);

    /// Takes the entities that match the game object and serializes them into a struct of arrays
    /// for fast loading.
    fn save_to_pack(
        entities: &[Entity],
        queries: &Queries<Everything>,
        mapping: &crate::scene::EntityMap,
        assets: &Assets,
    ) -> Self::Pack;

    /// After the struct of arrays has been deserialized, this function takes the descriptors and
    /// uses them to construct the components
    fn load_from_pack(
        pack: Self::Pack,
        entities: &crate::scene::EntityMap,
        assets: &Assets,
    ) -> Self;
}

#[macro_export]
macro_rules! count {
    () => (0usize);
    ( $x:tt $($xs:tt)* ) => (1usize + crate::count!($($xs)*));
}

#[macro_export]
macro_rules! game_object_def {
    ( $name:ident, $( $field:ty )+ ) => {
        paste::paste! {
            #[derive(Default)]
            #[allow(non_snake_case)]
            pub struct $name {
                $(
                    pub [<field_ $field>]: Vec<$field>,
                )*
            }

            #[derive(Default, Serialize, Deserialize)]
            #[allow(non_snake_case)]
            pub struct [<$name Pack>] {
                pub entities: Vec<crate::scene::MappedEntity>,
                $(
                    pub [<field_ $field>]: Vec<<$field as crate::serialization::SaveLoad>::Descriptor>,
                )*
            }
        }

        impl crate::object::GameObject for $name {
            type Pack = paste::paste! { [<$name Pack>] };

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

            fn instantiate(self, entities: &[ard_ecs::prelude::Entity], commands: &ard_ecs::prelude::EntityCommands) {
                paste::paste! {
                    let pack = (
                        $(
                            self.[<field _$field>],
                        )*
                    );
                }

                commands.set_components(entities, pack);
            }

            fn save_to_pack(
                entities: &[ard_ecs::prelude::Entity],
                queries: &ard_ecs::prelude::Queries<ard_ecs::prelude::Everything>,
                mapping: &crate::scene::EntityMap,
                assets: &ard_assets::manager::Assets,
            ) -> Self::Pack {
                use crate::serialization::{SaveLoad};

                let mut descriptor = Self::Pack::default();
                descriptor.entities = Vec::with_capacity(entities.len());

                for entity in entities {
                    descriptor.entities.push(mapping.to_map(*entity));
                }

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
                mut descriptor: Self::Pack,
                entities: &crate::scene::EntityMap,
                assets: &ard_assets::manager::Assets,
            ) -> Self {
                let mut components = Self::default();

                // Take the descriptors and convert them into components
                paste::paste! {$(
                    let mem = std::mem::take(&mut descriptor.[<field_ $field>]);
                    components.[<field_ $field>].reserve_exact(mem.len());
                    for elem in mem {
                        components.[<field_ $field>].push(
                            <$field as crate::serialization::SaveLoad>::load(
                                elem,
                                entities,
                                assets
                            )
                        );
                    }
                )*}

                components
            }
        }
    };
}
