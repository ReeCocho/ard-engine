use std::{any::TypeId, collections::HashMap};

use paste::paste;

use crate::{
    archetype::{storage::AnyArchetypeStorage, Archetype, ArchetypeId, Archetypes},
    component::Component,
    entity::Entity,
    key::TypeKey,
};

/// A component pack holds a set of components for one or more entities.
pub trait ComponentPack: Send + Sync {
    fn is_valid(&self) -> bool;

    /// Gets the number of entities represented by the components in the pack. Panics if the pack
    /// is invalid.
    fn len(&self) -> usize;

    /// Panics if the pack is invalid.
    fn is_empty(&self) -> bool;

    /// Generate the type key for the components within the pack.
    fn type_key(&self) -> TypeKey;

    /// Moves all of the components in the pack into archetype storage along with their associated
    /// entities.
    ///
    /// Returns the ID of the archetype the components are contained within and the beginning
    /// index within each buffer the components were inserted at (in that order).
    ///
    /// Panics if the pack isn't valid or if the pack length and entity slice length mismatch.
    fn move_into(
        &mut self,
        entities: &[Entity],
        archetypes: &mut Archetypes,
    ) -> (ArchetypeId, usize);
}

pub(crate) struct EmptyComponentPack {
    pub count: usize,
}

/// Implementation of component pack for empty tuple. Used for entity creation without components.
impl ComponentPack for EmptyComponentPack {
    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn len(&self) -> usize {
        self.count
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    fn move_into(
        &mut self,
        entities: &[Entity],
        archetypes: &mut Archetypes,
    ) -> (ArchetypeId, usize) {
        assert!(self.is_valid());
        assert!(entities.len() >= self.len());

        // Get the archetype for the pack
        let type_key = self.type_key();
        let (archetype, index) = if let Some(archetype) = archetypes.get_archetype(&type_key) {
            archetype
        } else {
            // Archetype doesn't exist, so we need to make one
            let mut archetype = Archetype {
                type_key: type_key.clone(),
                map: HashMap::default(),
                entities: 0,
            };

            // Create entity buffer
            archetype.entities = archetypes.get_entity_storage_mut().create();

            // Create the archetype
            archetypes.add_archetype(archetype);

            // Safe to unwrap since we just added it
            archetypes.get_archetype(&type_key).unwrap()
        };

        let archetype: &Archetype = archetype;

        // Move all entities into the entity buffer (and store the beginning index for the entities)
        let begin_ind = {
            let mut entity_buffer = archetypes.get_entity_storage().get_mut(archetype.entities);
            let begin = entity_buffer.len();
            entity_buffer.extend_from_slice(entities);
            begin
        };

        // Return index of the archetype and beginning index within the buffer
        (index, begin_ind)
    }

    #[inline]
    fn type_key(&self) -> TypeKey {
        TypeKey::default()
    }
}

/// Macro to help implement the `ComponentPack` trait for tuples of component vectors.
macro_rules! component_pack_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        /// Implementation for a tuple of vectors of components.
        impl<$($name: Component + 'static, )*> ComponentPack for ($(Vec<$name>,)*) {
            #[inline]
            fn is_valid(&self) -> bool {
                paste! {
                    #[allow(non_snake_case)]
                    let ($([<$name _ref>],)*) = self;
                }

                let len = self.0.len();
                paste! {$(
                    if [<$name _ref>].len() != len {
                        return false;
                    }
                )*}

                true
            }

            #[inline]
            fn len(&self) -> usize {
                debug_assert!(self.is_valid());
                self.0.len()
            }

            #[inline]
            fn is_empty(&self) -> bool {
                debug_assert!(self.is_valid());
                self.0.is_empty()
            }

            #[inline]
            fn type_key(&self) -> TypeKey {
                let mut type_key = TypeKey::default();
                $(
                    type_key.add::<$name>();
                )*
                type_key
            }

            fn move_into(
                &mut self,
                entities: &[Entity],
                archetypes: &mut Archetypes,
            ) -> (ArchetypeId, usize)
            {
                debug_assert!(self.is_valid());
                debug_assert!(entities.len() >= self.len());

                // Get the archetype for the pack
                let type_key = self.type_key();
                let (archetype, index) = if let Some(archetype) = archetypes.get_archetype(&type_key) {
                    archetype
                } else {
                    // Archetype doesn't exist, so we need to make one
                    let mut archetype = Archetype {
                        type_key: type_key.clone(),
                        map: HashMap::default(),
                        entities: 0,
                    };

                    // Initialize map values
                    $(
                        let storage = if let Some(storage) = archetypes
                            .get_storage_mut::<$name>()
                        {
                            storage
                        } else {
                            archetypes.create_storage::<$name>();
                            // Safe to unwrap since we just created it
                            archetypes.get_storage_mut::<$name>().unwrap()
                        };
                        archetype.map.insert(TypeId::of::<$name>(), storage.create());
                    )*

                    // Create entity buffer
                    archetype.entities = archetypes.get_entity_storage_mut().create();

                    // Create the archetype
                    archetypes.add_archetype(archetype);

                    // Safe to unwrap since we just added it
                    archetypes.get_archetype(&type_key).unwrap()
                };

                let archetype: &Archetype = archetype;

                // Move all entities into the entity buffer (and store the beginning index for the entities)
                let begin_ind = {
                    let mut entity_buffer =
                        archetypes.get_entity_storage().get_mut(archetype.entities);
                    let begin = entity_buffer.len();
                    entity_buffer.extend_from_slice(entities);
                    begin
                };

                // Decompose the tuple
                paste! {
                    #[allow(non_snake_case)]
                    let ($([<$name _ref>],)*) = self;
                }

                // Move all components into their respective buffers
                paste!{$(
                    let ind = *archetype.map
                        .get(&TypeId::of::<$name>())
                        .expect("Archetype map missing component type in pack.");
                    let mut buffer = archetypes
                        .get_storage::<$name>()
                        .expect("Component storage missing index.").get_mut(ind);
                    for component in [<$name _ref>].drain(..) {
                        buffer.push(component);
                    }
                )*}

                // Return index of the archetype and beginning index within the buffer
                (index, begin_ind)
            }
        }
    }
}

component_pack_impl! { 1, A }
component_pack_impl! { 2, A B }
component_pack_impl! { 3, A B C }
component_pack_impl! { 4, A B C D }
component_pack_impl! { 5, A B C D E }
component_pack_impl! { 6, A B C D E F }
component_pack_impl! { 7, A B C D E F G }
component_pack_impl! { 8, A B C D E F G H }
component_pack_impl! { 9, A B C D E F G H I }
component_pack_impl! { 10, A B C D E F G H I J }
component_pack_impl! { 11, A B C D E F G H I J K }
component_pack_impl! { 12, A B C D E F G H I J K L }
