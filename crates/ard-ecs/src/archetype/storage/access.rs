use std::ptr::NonNull;

use unsafe_unwrap::UnsafeUnwrap;

use crate::{
    archetype::Archetypes,
    component::{access::ComponentAccess, Component},
    prw_lock::{PrwReadLock, PrwWriteLock},
};

/// A way to access a buffer within an archetype storage.
pub trait StorageBufferAccess: Default {
    /// Type of component held in the buffer.
    type Component: Component;

    /// Access type for the held object.
    type ComponentAccess: ComponentAccess;

    /// Creates an instance of the storage. Finds the appropriate storage type by component
    /// and uses the Nth one via the index provided.
    fn new(archetypes: &Archetypes, index: usize) -> Self;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;

    /// Determines if the provided index is valid.
    fn is_valid(&self, idx: usize) -> bool;

    /// Fetch a component in the buffer by index.
    ///
    /// # Safety
    /// No bounds checking is performed. It is left up to the caller to not read out of bounds.
    unsafe fn fetch(&self, idx: usize) -> Self::ComponentAccess;
}

// Sad time raw pointers :(
// I made an attempt to borrow the slice in the lock, but we had the possibility of multiple
// mutable references, so that didn't work. The iteration technique is super fast though.

pub struct ReadStorageBuffer<T> {
    #[allow(dead_code)]
    handle: Option<PrwReadLock<Vec<T>>>,
    ptr: NonNull<T>,
    len: usize,
}

pub struct WriteStorageBuffer<T> {
    #[allow(dead_code)]
    handle: Option<PrwWriteLock<Vec<T>>>,
    ptr: NonNull<T>,
    len: usize,
}

impl<T: Component + 'static> StorageBufferAccess for ReadStorageBuffer<T> {
    type Component = T;
    type ComponentAccess = &'static Self::Component;

    #[inline]
    fn new(archetypes: &Archetypes, index: usize) -> Self {
        let handle = archetypes
            .get_storage::<Self::Component>()
            .expect("Requested non existant storage")
            .get(index);
        let len = handle.len();
        let ptr = handle.as_ptr();

        Self {
            handle: Some(handle),
            len,
            ptr: if ptr.is_null() {
                unsafe {
                    NonNull::new_unchecked(
                        std::mem::size_of::<T>().max(std::mem::size_of::<usize>()) as *mut T,
                    )
                }
            } else {
                unsafe { NonNull::new_unchecked(ptr as *mut T) }
            },
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn is_valid(&self, idx: usize) -> bool {
        idx < self.len
    }

    #[inline]
    unsafe fn fetch(&self, idx: usize) -> Self::ComponentAccess {
        self.ptr.as_ptr().add(idx).as_ref().unsafe_unwrap()
    }
}

impl<T> Default for ReadStorageBuffer<T> {
    #[inline]
    fn default() -> Self {
        Self {
            handle: None,
            len: 0,
            ptr: unsafe {
                NonNull::new_unchecked(
                    std::mem::size_of::<T>().max(std::mem::size_of::<usize>()) as *mut T
                )
            },
        }
    }
}

unsafe impl<T> Send for ReadStorageBuffer<T> {}

unsafe impl<T> Sync for ReadStorageBuffer<T> {}

impl<T: Component + 'static> StorageBufferAccess for WriteStorageBuffer<T> {
    type Component = T;
    type ComponentAccess = &'static mut T;

    #[inline]
    fn new(archetypes: &Archetypes, index: usize) -> Self {
        let mut handle = archetypes
            .get_storage::<Self::Component>()
            .expect("Requested non existant storage")
            .get_mut(index);
        let len = handle.len();
        let ptr = handle.as_mut_ptr();

        Self {
            handle: Some(handle),
            len,
            ptr: if ptr.is_null() {
                unsafe {
                    NonNull::new_unchecked(
                        std::mem::size_of::<T>().max(std::mem::size_of::<usize>()) as *mut T,
                    )
                }
            } else {
                unsafe { NonNull::new_unchecked(ptr) }
            },
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn is_valid(&self, idx: usize) -> bool {
        idx < self.len
    }

    #[inline]
    unsafe fn fetch(&self, idx: usize) -> Self::ComponentAccess {
        self.ptr.as_ptr().add(idx).as_mut().unsafe_unwrap()
    }
}

impl<T> Default for WriteStorageBuffer<T> {
    #[inline]
    fn default() -> Self {
        Self {
            handle: None,
            len: 0,
            ptr: unsafe {
                NonNull::new_unchecked(
                    std::mem::size_of::<T>().max(std::mem::size_of::<usize>()) as *mut T
                )
            },
        }
    }
}

unsafe impl<T> Send for WriteStorageBuffer<T> {}

unsafe impl<T> Sync for WriteStorageBuffer<T> {}
