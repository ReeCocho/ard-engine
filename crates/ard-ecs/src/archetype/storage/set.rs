use paste::paste;

use crate::{archetype::storage::access::StorageBufferAccess, component::filter::ComponentFilter};

/// A set of archetype storages.
pub trait ArchetypeStorageSet: Default {
    /// Component filter associated with the set.
    type Filter: ComponentFilter;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;

    /// Determines if the provided index points to valid components
    fn is_valid(&self, idx: usize) -> bool;

    /// Fetch a filter in the storage by index.
    ///
    /// # Safety
    /// No bounds check is performed. It is up to the caller to ensure the set is valid.
    unsafe fn fetch(&mut self, idx: usize) -> Self::Filter;
}

macro_rules! archetype_storage_set_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        impl<$($name: StorageBufferAccess,)*> ArchetypeStorageSet for ($($name,)*) {
            type Filter = ($($name::ComponentAccess,)*);

            #[inline]
            fn len(&self) -> usize {
                self.0.len()
            }

            #[inline]
            fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            #[inline]
            fn is_valid(&self, idx: usize) -> bool {
                self.0.is_valid(idx)
            }

            #[inline]
            unsafe fn fetch(&mut self, idx: usize) -> Self::Filter {
                paste! {
                    #[allow(non_snake_case)]
                    let ($([<$name _storage>],)*) = self;
                }

                paste! { ($(
                    [<$name _storage>].fetch(idx),
                )*) }
            }
        }
    }
}

archetype_storage_set_impl! { 1, A }
archetype_storage_set_impl! { 2, A B }
archetype_storage_set_impl! { 3, A B C }
archetype_storage_set_impl! { 4, A B C D }
archetype_storage_set_impl! { 5, A B C D E }
archetype_storage_set_impl! { 6, A B C D E F }
archetype_storage_set_impl! { 7, A B C D E F G }
archetype_storage_set_impl! { 8, A B C D E F G H }
archetype_storage_set_impl! { 9, A B C D E F G H I }
archetype_storage_set_impl! { 10, A B C D E F G H I J }
archetype_storage_set_impl! { 11, A B C D E F G H I J K }
archetype_storage_set_impl! { 12, A B C D E F G H I J K L }
