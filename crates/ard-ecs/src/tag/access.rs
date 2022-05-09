use crate::tag::{
    storage::access::{ReadTagStorage, TagStorageAccess, WriteTagStorage},
    Tag, Tags,
};

/// Represents a way to access a particular tag type.
pub trait TagAccess {
    /// The tag type being accessed.
    type Tag: Tag + 'static;

    /// The type of storage buffer access needed for the tag access.
    type StorageAccess: TagStorageAccess;

    /// Indicates that the tag access type is mutable.
    const MUT_ACCESS: bool;

    /// Get the storage for the tag.
    ///
    /// Returns `None` if the storage doesn't exist.
    fn get_storage(tags: &Tags) -> Self::StorageAccess;
}

impl<'a, T: Tag + 'static> TagAccess for &'a T {
    type Tag = T;
    type StorageAccess = ReadTagStorage<T::Storage, T>;
    const MUT_ACCESS: bool = false;

    #[inline]
    fn get_storage(tags: &Tags) -> Self::StorageAccess {
        Self::StorageAccess::from_tags(tags)
    }
}

impl<'a, T: Tag + 'static> TagAccess for &'a mut T {
    type Tag = T;
    type StorageAccess = WriteTagStorage<T::Storage, T>;
    const MUT_ACCESS: bool = true;

    #[inline]
    fn get_storage(tags: &Tags) -> Self::StorageAccess {
        Self::StorageAccess::from_tags(tags)
    }
}
