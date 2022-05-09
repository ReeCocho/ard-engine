use crate::{
    archetype::storage::access::{ReadStorageBuffer, WriteStorageBuffer},
    component::access::ComponentAccess,
    prelude::{
        storage::access::{ReadTagStorage, TagStorageAccess, WriteTagStorage},
        Component, Resource, Resources, Tag, Tags,
    },
    prw_lock::{PrwReadLock, PrwWriteLock},
    resource::access::ResourceAccess,
    tag::access::TagAccess,
};

/// Requests read access for some resource.
pub struct Read<T> {
    _phantom: std::marker::PhantomData<T>,
}

/// Requests write access for some resource.
pub struct Write<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Tag + 'static> TagAccess for Read<T> {
    type Tag = T;
    type StorageAccess = ReadTagStorage<T::Storage, T>;
    const MUT_ACCESS: bool = false;

    #[inline]
    fn get_storage(tags: &Tags) -> Self::StorageAccess {
        Self::StorageAccess::from_tags(tags)
    }
}

impl<T: Tag + 'static> TagAccess for Write<T> {
    type Tag = T;
    type StorageAccess = WriteTagStorage<T::Storage, T>;
    const MUT_ACCESS: bool = true;

    #[inline]
    fn get_storage(tags: &Tags) -> Self::StorageAccess {
        Self::StorageAccess::from_tags(tags)
    }
}

impl<C: Component + 'static> ComponentAccess for Read<C> {
    type Component = C;
    type Storage = ReadStorageBuffer<C>;
    const MUT_ACCESS: bool = false;
}

impl<C: Component + 'static> ComponentAccess for Write<C> {
    type Component = C;
    type Storage = WriteStorageBuffer<C>;
    const MUT_ACCESS: bool = true;
}

impl<R: Resource + 'static> ResourceAccess for Read<R> {
    type Resource = R;
    type Lock = PrwReadLock<R>;
    const MUT_ACCESS: bool = false;

    fn get_lock(resources: &Resources) -> Option<Self::Lock> {
        resources.get::<R>()
    }
}

impl<R: Resource + 'static> ResourceAccess for Write<R> {
    type Resource = R;
    type Lock = PrwWriteLock<R>;
    const MUT_ACCESS: bool = true;

    fn get_lock(resources: &Resources) -> Option<Self::Lock> {
        resources.get_mut::<R>()
    }
}
