use paste::paste;

use crate::{
    entity::Entity,
    key::TypeKey,
    tag::{storage::TagStorage, Tag, TagCollection, TagCollectionId, Tags},
};

/// A tag pack holds a set of tags for one or more entities.
pub trait TagPack: Send + Sync {
    fn is_valid(&self) -> bool;

    /// Gets the number of entities represented by tags components in the pack. Panics if the pack
    /// is invalid.
    fn len(&self) -> usize;

    /// Panics if the pack is invalid.
    fn is_empty(&self) -> bool;

    /// Moves all of the tags in the pack into tag storages along with their associated
    /// entities. Returns the ID of the collection the tag set belongs to.
    ///
    /// # Panics
    /// Panics if the pack isn't valid or if the pack length and entity slice length mismatch.
    fn move_into(&mut self, entities: &[Entity], tags: &mut Tags) -> TagCollectionId;

    /// Generate the type kru for the tags within the pack.
    fn type_key(&self) -> TypeKey;
}

/// Implementation of tag pack for empty tuple. Used for entity creation without tags.
impl TagPack for () {
    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn len(&self) -> usize {
        0
    }

    #[inline]
    fn is_empty(&self) -> bool {
        true
    }

    #[inline]
    fn move_into(&mut self, _: &[Entity], _: &mut Tags) -> TagCollectionId {
        TagCollectionId::default()
    }

    #[inline]
    fn type_key(&self) -> TypeKey {
        TypeKey::default()
    }
}

macro_rules! tag_pack_impl {
    ( $n:expr, $( $name:ident )+ ) => {
        /// Implementation for a tuple of vectors of tags.
        impl<$($name: Tag + 'static, )*> TagPack for ($(Vec<$name>,)*) {
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
                assert!(self.is_valid());
                self.0.len()
            }

            #[inline]
            fn is_empty(&self) -> bool {
                assert!(self.is_valid());
                self.0.is_empty()
            }

            fn move_into(
                &mut self,
                entities: &[Entity],
                tags: &mut Tags,
            ) -> TagCollectionId
            {
                assert!(self.is_valid());

                // Decompose the tuple
                paste! {
                    #[allow(non_snake_case)]
                    let ($([<$name _ref>],)*) = self;
                }

                // Move all tags into their respective storages
                paste!{$({
                    let mut storage = tags.get_storage_or_default_mut::<$name>();
                    for (i, tag) in [<$name _ref>].drain(..).enumerate() {
                        storage.add(entities[i], tag);
                    }
                })*}

                // Generate the type key for the pack
                let type_key = self.type_key();

                // Either return the ID of the collection or create a new one if it doesn't yet
                // exist
                if let Some(id) = tags.get_collection_id(&type_key) {
                    id
                } else {
                    // Create and add collection
                    tags.add_collection(TagCollection {
                        type_key,
                    })
                }
            }

            #[inline]
            fn type_key(&self) -> TypeKey {
                let mut type_key = TypeKey::default();
                $(
                    type_key.add::<$name>();
                )*
                type_key
            }
        }
    }
}

tag_pack_impl! { 1, A }
tag_pack_impl! { 2, A B }
tag_pack_impl! { 3, A B C }
tag_pack_impl! { 4, A B C D }
tag_pack_impl! { 5, A B C D E }
tag_pack_impl! { 6, A B C D E F }
tag_pack_impl! { 7, A B C D E F G }
tag_pack_impl! { 8, A B C D E F G H }
tag_pack_impl! { 9, A B C D E F G H I }
tag_pack_impl! { 10, A B C D E F G H I J }
tag_pack_impl! { 11, A B C D E F G H I J K }
tag_pack_impl! { 12, A B C D E F G H I J K L }
