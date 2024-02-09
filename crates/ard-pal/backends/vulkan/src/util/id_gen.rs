use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

#[derive(Debug)]
pub(crate) struct IdGenerator {
    counter: AtomicU32,
    free: Mutex<Vec<ResourceId>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResourceId(NonZeroU32);

impl Default for IdGenerator {
    fn default() -> Self {
        Self {
            counter: AtomicU32::new(1),
            free: Mutex::new(Vec::default()),
        }
    }
}

impl IdGenerator {
    #[inline(always)]
    pub fn create(&self) -> ResourceId {
        match self.free.lock().unwrap().pop() {
            Some(id) => id,
            // SAFETY: Safe since we initialize the counter to 1.
            None => ResourceId(unsafe {
                let id = self.counter.fetch_add(1, Ordering::Relaxed);
                debug_assert!(id != 0);
                NonZeroU32::new_unchecked(id)
            }),
        }
    }

    #[inline(always)]
    pub fn free(&self, id: ResourceId) {
        self.free.lock().unwrap().push(id);
    }
}

impl ResourceId {
    #[inline(always)]
    pub fn as_idx(self) -> usize {
        (self.0.get() - 1) as usize
    }
}
