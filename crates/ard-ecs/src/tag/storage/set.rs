use paste::paste;

use crate::{
    entity::Entity,
    tag::{storage::access::TagStorageAccess, Tags},
};

/// A tag storage set is used to hold the appropriate storages for a particular tag filter.
pub trait TagStorageSet {
    /// Associated set of optional tag accesses.
    type TagSet;

    /// Creates an instance of the storage set given the tags to read from.
    fn from_tags(tags: &Tags) -> Self;

    /// Creates an instance of the tag set given an entity.
    fn make_set(&self, entity: Entity) -> Self::TagSet;
}

impl TagStorageSet for () {
    type TagSet = ();

    #[inline]
    fn from_tags(_: &Tags) -> Self {}

    #[inline]
    fn make_set(&self, _: Entity) -> Self::TagSet {}
}

impl<T: TagStorageAccess> TagStorageSet for T {
    type TagSet = Option<T::TagAccess>;

    #[inline]
    fn from_tags(tags: &Tags) -> Self {
        T::from_tags(tags)
    }

    #[inline]
    fn make_set(&self, entity: Entity) -> Self::TagSet {
        self.get_tag(entity)
    }
}

macro_rules! tag_storage_set_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        impl<$($name: TagStorageAccess,)*> TagStorageSet for ($($name,)*) {
            type TagSet = ($(Option<$name::TagAccess>,)*);

            #[inline]
            fn from_tags(tags: &Tags) -> Self {
                ( $(
                    $name::from_tags(tags),
                )* )
            }

            #[inline]
            fn make_set(&self, entity: Entity) -> Self::TagSet {
                paste! {
                    #[allow(non_snake_case)]
                    let ($([<$name _storage>],)*) = self;
                }

                paste! { ($(
                    [<$name _storage>].get_tag(entity),
                )*) }
            }
        }
    }
}

tag_storage_set_impl! { 1, A }
tag_storage_set_impl! { 2, A B }
tag_storage_set_impl! { 3, A B C }
tag_storage_set_impl! { 4, A B C D }
tag_storage_set_impl! { 5, A B C D E }
tag_storage_set_impl! { 6, A B C D E F }
tag_storage_set_impl! { 7, A B C D E F G }
tag_storage_set_impl! { 8, A B C D E F G H }
tag_storage_set_impl! { 9, A B C D E F G H I }
tag_storage_set_impl! { 10, A B C D E F G H I J }
tag_storage_set_impl! { 11, A B C D E F G H I J K }
tag_storage_set_impl! { 12, A B C D E F G H I J K L }
