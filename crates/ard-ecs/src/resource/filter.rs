use crate::{
    key::TypeKey,
    resource::{access::ResourceAccess, Resources},
};

/// Represents a set of resources used by a system.
pub trait ResourceFilter {
    /// Tuple containing the requested resources.
    type Set;

    /// Gets the type key of all resources read within the filter.
    fn type_key() -> TypeKey;

    /// Gets the type key of resources read using the filter.
    fn read_type_key() -> TypeKey;

    /// Gets the type key of resources mutated using the filter.
    fn mut_type_key() -> TypeKey;

    /// Gets the resources from a resource container.
    ///
    /// # Panics
    /// Panics if the XOR borrowing rules are broken for the requested resources.
    fn get(resources: &Resources) -> Self::Set;
}

impl ResourceFilter for () {
    type Set = ();

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

    #[inline]
    fn get(_: &Resources) -> Self::Set {}
}

macro_rules! resource_set_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        impl<$($name: ResourceAccess + 'static,)*> ResourceFilter for ($($name,)*) {
            type Set = ($(Option<$name::Lock>,)*);

            #[inline]
            fn type_key() -> TypeKey {
                let mut set = TypeKey::default();
                $(
                    set.add::<$name>();
                )*
                set
            }

            #[inline]
            fn read_type_key() -> TypeKey {
                let mut set = TypeKey::default();
                $(
                    if !$name::MUT_ACCESS {
                        set.add::<$name>();
                    }
                )*
                set
            }

            #[inline]
            fn mut_type_key() -> TypeKey {
                let mut set = TypeKey::default();
                $(
                    if $name::MUT_ACCESS {
                        set.add::<$name>();
                    }
                )*
                set
            }

            #[inline]
            fn get(resources: &Resources) -> Self::Set {
                ( $(
                    $name::get_lock(resources),
                )* )
            }
        }
    }
}

resource_set_impl! { 1, A }
resource_set_impl! { 2, A B }
resource_set_impl! { 3, A B C }
resource_set_impl! { 4, A B C D }
resource_set_impl! { 5, A B C D E }
resource_set_impl! { 6, A B C D E F }
resource_set_impl! { 7, A B C D E F G }
resource_set_impl! { 8, A B C D E F G H }
resource_set_impl! { 9, A B C D E F G H I }
resource_set_impl! { 10, A B C D E F G H I J }
resource_set_impl! { 11, A B C D E F G H I J K }
resource_set_impl! { 12, A B C D E F G H I J K L }
