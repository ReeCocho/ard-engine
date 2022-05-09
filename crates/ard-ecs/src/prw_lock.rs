use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use unsafe_unwrap::UnsafeUnwrap;

/// Panicy Read/Write Lock
///
/// Behaves the same as a normal RwLock, except the handles to the interior data have no lifetime.
/// The consequence of this is that if the lock is dropped before any handles, a panic will occur.
#[derive(Debug)]
pub struct PrwLock<T>(Arc<PrwLockInner<T>>);

#[derive(Debug)]
pub struct PrwLockInner<T> {
    data: UnsafeCell<T>,
    state: AccessState,
}

#[derive(Debug)]
pub struct PrwReadLock<T> {
    lock: Arc<PrwLockInner<T>>,
}

#[derive(Debug)]
pub struct PrwWriteLock<T> {
    lock: Arc<PrwLockInner<T>>,
}

#[derive(Debug, Default)]
struct AccessState {
    write: AtomicBool,
    read: AtomicUsize,
}

impl<T> PrwLock<T> {
    pub fn new(data: T) -> Self {
        Self(Arc::new(PrwLockInner {
            data: UnsafeCell::new(data),
            state: AccessState::default(),
        }))
    }

    pub fn read(&self) -> PrwReadLock<T> {
        if self.0.state.write.load(Ordering::Relaxed) {
            panic!("read access requested when there is already a write request");
        }
        self.0.state.read.fetch_add(1, Ordering::Relaxed);

        PrwReadLock::new(self.0.clone())
    }

    pub fn write(&self) -> PrwWriteLock<T> {
        if self.0.state.write.load(Ordering::Relaxed)
            || self.0.state.read.load(Ordering::Relaxed) > 0
        {
            panic!("write access requested for when there is already a write/read request");
        }
        self.0.state.write.store(true, Ordering::Relaxed);

        PrwWriteLock::new(self.0.clone())
    }
}

impl<T> Drop for PrwLock<T> {
    fn drop(&mut self) {
        // Panic if there are any outstanding access handles
        if self.0.state.write.load(Ordering::Relaxed)
            || self.0.state.read.load(Ordering::Relaxed) > 0
        {
            panic!("outstanding access handle in archetype storage on drop");
        }
    }
}

unsafe impl<T> Send for PrwLockInner<T> {}

unsafe impl<T> Sync for PrwLockInner<T> {}

impl<T> PrwReadLock<T> {
    fn new(lock: Arc<PrwLockInner<T>>) -> Self {
        Self { lock }
    }
}

impl<T> Drop for PrwReadLock<T> {
    fn drop(&mut self) {
        self.lock.state.read.fetch_sub(1, Ordering::Relaxed);
    }
}

impl<T> Deref for PrwReadLock<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.lock.data.get().as_ref().unsafe_unwrap() }
    }
}

unsafe impl<T> Send for PrwReadLock<T> {}

unsafe impl<T> Sync for PrwReadLock<T> {}

impl<T> PrwWriteLock<T> {
    fn new(lock: Arc<PrwLockInner<T>>) -> Self {
        Self { lock }
    }
}

impl<T> Drop for PrwWriteLock<T> {
    fn drop(&mut self) {
        self.lock.state.write.store(false, Ordering::Relaxed);
    }
}

impl<T> Deref for PrwWriteLock<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.lock.data.get().as_ref().unsafe_unwrap() }
    }
}

impl<T> DerefMut for PrwWriteLock<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safe to unwrap because the PrwWriteLock guarantees there are no other references
        unsafe { self.lock.data.get().as_mut().unsafe_unwrap() }
    }
}

unsafe impl<T> Send for PrwWriteLock<T> {}

unsafe impl<T> Sync for PrwWriteLock<T> {}
