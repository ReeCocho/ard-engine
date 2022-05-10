use crate::{
    key::TypeKey,
    tag::{access::TagAccess, storage::set::TagStorageSet},
};

/// A tag filter represents a set of tags and how we wish to access them (read only or read/write).
pub trait TagFilter
where
    Self: Sized,
{
    /// Storage set associated with the filter.
    type StorageSet: TagStorageSet;

    /// Generates the type key all tags.
    fn type_key() -> TypeKey;

    /// Generates the type key of read only tags.
    fn read_type_key() -> TypeKey;

    /// Generates the type key for mutable tags.
    fn mut_type_key() -> TypeKey;
}

impl TagFilter for () {
    type StorageSet = ();

    #[inline]
    fn type_key() -> TypeKey {
        TypeKey::default()
    }

    #[inline]
    fn read_type_key() -> TypeKey {
        TypeKey::default()
    }

    #[inline]
    fn mut_type_key() -> TypeKey {
        TypeKey::default()
    }
}

macro_rules! tag_filter_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        impl<$($name: TagAccess,)*> TagFilter for ($($name,)*) {
            type StorageSet = ($($name::StorageAccess,)*);

            #[inline]
            fn type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    descriptor.add::<$name::Tag>();
                )*
                descriptor
            }

            #[inline]
            fn read_type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    if !$name::MUT_ACCESS {
                        descriptor.add::<$name::Tag>();
                    }
                )*
                descriptor
            }

            #[inline]
            fn mut_type_key() -> TypeKey {
                let mut descriptor = TypeKey::default();
                $(
                    if $name::MUT_ACCESS {
                        descriptor.add::<$name::Tag>();
                    }
                )*
                descriptor
            }
        }
    }
}

tag_filter_impl! { 1, A }
tag_filter_impl! { 2, A B }
tag_filter_impl! { 3, A B C }
tag_filter_impl! { 4, A B C D }
tag_filter_impl! { 5, A B C D E }
tag_filter_impl! { 6, A B C D E F }
tag_filter_impl! { 7, A B C D E F G }
tag_filter_impl! { 8, A B C D E F G H }
tag_filter_impl! { 9, A B C D E F G H I }
tag_filter_impl! { 10, A B C D E F G H I J }
tag_filter_impl! { 11, A B C D E F G H I J K }
tag_filter_impl! { 12, A B C D E F G H I J K L }
