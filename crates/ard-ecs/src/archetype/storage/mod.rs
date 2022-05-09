pub mod access;
pub mod set;

use std::any::Any;

use crate::prw_lock::{PrwLock, PrwReadLock, PrwWriteLock};

/// Holds lists of components of a single type for archetypes.
#[derive(Debug, Default)]
pub struct ArchetypeStorage<T: Send + Sync> {
    /// The actual component buffers.
    ///
    /// We must box our vectors because access handles hold references to the vectors, so we must
    /// maintain a stable reference.
    buffers: Vec<PrwLock<Vec<T>>>,
}

pub trait AnyArchetypeStorage: Send + Sync {
    /// Converts the type into an any reference.
    fn as_any(&self) -> &dyn Any;

    /// Converts the type into a mutable any reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Requests a new buffer. Returns the index of the new buffer.
    fn create(&mut self) -> usize;

    /// Removes an object from the buffer and swaps the last element in the buffer with the
    /// destroyed object.
    ///
    /// # Panics
    /// Panics if the buffer index is out of bounds, if the object index is out of bounds, or if
    /// the buffer is currently requested for read or write.
    fn swap_remove(&mut self, buffer: usize, index: usize);

    /// Moves an object from a source buffer to a destination buffer and swaps the last element in
    /// the source buffer to take its place.
    fn swap_move(&mut self, dst_buffer: usize, src_buffer: usize, src_index: usize);

    /// Adds a new object to a buffer.
    fn add(&mut self, object: Box<dyn Any>, buffer: usize);

    /// Replaces an object in a buffer with a new one.
    fn replace(&mut self, object: Box<dyn Any>, buffer: usize, index: usize);
}

impl<T: Send + Sync> ArchetypeStorage<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
        }
    }

    /// Requests immutable access to a buffer within the storage.
    ///
    /// Panics if the buffer is currently being read from/written to or if the providd index is
    /// invalid.
    #[inline]
    pub fn get(&self, i: usize) -> PrwReadLock<Vec<T>> {
        self.buffers[i].read()
    }

    /// Requests mutable access to a buffer within the storage.
    ///
    /// Panics if the buffer is currently being read from/written to or if the providd index is
    /// invalid.
    #[inline]
    pub fn get_mut(&self, i: usize) -> PrwWriteLock<Vec<T>> {
        self.buffers[i].write()
    }
}

impl<T: Send + Sync + 'static> AnyArchetypeStorage for ArchetypeStorage<T> {
    #[inline]
    fn create(&mut self) -> usize {
        self.buffers.push(PrwLock::new(Vec::default()));
        self.buffers.len() - 1
    }

    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline]
    fn add(&mut self, object: Box<dyn Any>, buffer: usize) {
        self.buffers[buffer].write().push(
            *object
                .downcast::<T>()
                .expect("wrong object type provided to storage for add"),
        );
    }

    #[inline]
    fn swap_remove(&mut self, buffer: usize, index: usize) {
        self.buffers[buffer].write().swap_remove(index);
    }

    #[inline]
    fn swap_move(&mut self, dst_buffer: usize, src_buffer: usize, src_index: usize) {
        self.buffers[dst_buffer]
            .write()
            .push(self.buffers[src_buffer].write().swap_remove(src_index));
    }

    #[inline]
    fn replace(&mut self, object: Box<dyn Any>, buffer: usize, index: usize) {
        self.buffers[buffer].write()[index] = *object
            .downcast::<T>()
            .expect("wrong type for replacement in archetype storage");
    }
}
