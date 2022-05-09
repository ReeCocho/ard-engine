use std::any::TypeId;

use crate::{
    archetype::{
        storage::{access::StorageBufferAccess, set::ArchetypeStorageSet},
        Archetype, Archetypes,
    },
    component::access::ComponentAccess,
    key::TypeKey,
};

/// A component filter represents a set of components and how we wish to access them (read only
/// or read/write).
pub trait ComponentFilter
where
    Self: Sized,
{
    /// The appropriate storage set for the filter.
    type StorageSet: ArchetypeStorageSet;

    /// Generates the type key for the components within the filter given an registry.
    fn type_key() -> TypeKey;

    /// Generates the type key of read only components.
    fn read_type_key() -> TypeKey;

    /// Generates the bit set for mutable components.
    fn mut_type_key() -> TypeKey;

    /// Given an archetype, generates an instance of the storage set for the filter.
    ///
    /// Panics if the filter isn't a subset of the archetype.
    fn make_storage_set(archetype: &Archetype, archetypes: &Archetypes) -> Self::StorageSet;
}

macro_rules! component_filter_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        impl<$($name: ComponentAccess,)*> ComponentFilter for ($($name,)*) {
            type StorageSet = ($($name::Storage,)*);

            #[inline]
            fn type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    descriptor.add::<$name::Component>();
                )*
                descriptor
            }

            #[inline]
            fn read_type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    if !$name::MUT_ACCESS {
                        descriptor.add::<$name::Component>();
                    }
                )*
                descriptor
            }

            #[inline]
            fn mut_type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    if $name::MUT_ACCESS {
                        descriptor.add::<$name::Component>();
                    }
                )*
                descriptor
            }

            #[inline]
            fn make_storage_set(archetype: &Archetype, archetypes: &Archetypes)
                -> Self::StorageSet {
                ($(
                    $name::Storage::new(
                        archetypes,
                        *archetype.map.get(&TypeId::of::<$name::Component>()
                    ).expect("Provided archetype does not contain component in filter.")),
                )*)
            }
        }
    }
}

component_filter_impl! { 1, A }
component_filter_impl! { 2, A B }
component_filter_impl! { 3, A B C }
component_filter_impl! { 4, A B C D }
component_filter_impl! { 5, A B C D E }
component_filter_impl! { 6, A B C D E F }
component_filter_impl! { 7, A B C D E F G }
component_filter_impl! { 8, A B C D E F G H }
component_filter_impl! { 9, A B C D E F G H I }
component_filter_impl! { 10, A B C D E F G H I J }
component_filter_impl! { 11, A B C D E F G H I J K }
component_filter_impl! { 12, A B C D E F G H I J K L }
