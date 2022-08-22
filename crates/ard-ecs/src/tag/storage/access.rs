use std::ops::{Deref, DerefMut};

use crate::{
    entity::Entity,
    prw_lock::{PrwReadLock, PrwWriteLock},
    tag::{access::TagAccess, storage::TagStorage, Tag, Tags},
};

/// Represents a particular way to access a tag storage (read or read/write).
pub trait TagStorageAccess: Sized {
    /// Tag access associated with the storage.
    type TagAccess: TagAccess;

    /// Get the storage access from the tags container. Returns `None` if the storage doesn't
    /// exist in the tags container.
    fn from_tags(tags: &Tags) -> Self;

    /// Get a tag access from the storage. Returns `None` if the entity does not contain the tag.
    fn get_tag(&self, entity: Entity) -> Option<Self::TagAccess>;
}

// Sad time raw pointers :(
// Borrowing issues occur because of how tag retrieval works, so we use raw pointers to navigate
// around them. Not ideal. Should probably find a better way to do this.

pub struct ReadTagStorage<S: TagStorage<T>, T: Tag<Storage = S> + 'static> {
    lock: Option<PrwReadLock<S>>,
    storage: *const S,
    phantom: std::marker::PhantomData<T>,
}

pub struct WriteTagStorage<S: TagStorage<T>, T: Tag<Storage = S> + 'static> {
    lock: Option<PrwWriteLock<S>>,
    storage: *mut S,
    phantom: std::marker::PhantomData<T>,
}

impl<S: TagStorage<T> + 'static, T: Tag<Storage = S>> TagStorageAccess for ReadTagStorage<S, T> {
    type TagAccess = &'static T;

    #[inline]
    fn from_tags(tags: &Tags) -> Self {
        let lock = tags.get_storage::<T>();
        let storage = if let Some(lock) = &lock {
            lock.deref()
        } else {
            std::ptr::null::<S>()
        };

        Self {
            lock,
            storage,
            phantom: std::marker::PhantomData::<T>::default(),
        }
    }

    #[inline]
    fn get_tag(&self, entity: Entity) -> Option<Self::TagAccess> {
        if self.lock.is_some() {
            unsafe { (*self.storage).get(entity) }
        } else {
            None
        }
    }
}

impl<S: TagStorage<T> + 'static, T: Tag<Storage = S>> TagStorageAccess for WriteTagStorage<S, T> {
    type TagAccess = &'static mut T;

    #[inline]
    fn from_tags(tags: &Tags) -> Self {
        let mut lock = tags.get_storage_mut::<T>();
        let storage = if let Some(lock) = &mut lock {
            lock.deref_mut()
        } else {
            std::ptr::null_mut::<S>()
        };

        Self {
            lock,
            storage,
            phantom: std::marker::PhantomData::<T>::default(),
        }
    }

    #[inline]
    fn get_tag(&self, entity: Entity) -> Option<Self::TagAccess> {
        if self.lock.is_some() {
            unsafe { (*self.storage).get_mut(entity) }
        } else {
            None
        }
    }
}

unsafe impl<S: TagStorage<T>, T: Tag<Storage = S> + 'static> Send for ReadTagStorage<S, T> {}
unsafe impl<S: TagStorage<T>, T: Tag<Storage = S> + 'static> Sync for ReadTagStorage<S, T> {}

unsafe impl<S: TagStorage<T>, T: Tag<Storage = S> + 'static> Send for WriteTagStorage<S, T> {}
unsafe impl<S: TagStorage<T>, T: Tag<Storage = S> + 'static> Sync for WriteTagStorage<S, T> {}
