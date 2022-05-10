pub mod access;
pub mod filter;
pub mod res;

use anymap::{any::Any, AnyMap};
pub use ard_ecs_derive::Resource;

use crate::prw_lock::{PrwLock, PrwReadLock, PrwWriteLock};

/// A resource is a piece of data that can be shared between systems. Global state of a game, for
/// example, could be held in a resource.
pub trait Resource {}

/// Container of resources.
pub struct Resources {
    resources: AnyMap,
}

unsafe impl Send for Resources {}

unsafe impl Sync for Resources {}

impl Default for Resources {
    fn default() -> Self {
        Resources {
            resources: AnyMap::new(),
        }
    }
}

impl Resources {
    pub fn new() -> Resources {
        Resources::default()
    }

    /// Creates a new resource. If a resource of the same type was already registered, it is
    /// replaced by the new resource.
    pub fn add<R: Resource + Any>(&mut self, resource: R) {
        self.resources.insert(PrwLock::new(resource));
    }

    /// Checks that a resource exists in the container.
    pub fn contains<R: Resource + Any>(&self) -> bool {
        self.resources.contains::<PrwLock<R>>()
    }

    /// Attempts to get read only access to a resource.
    pub fn get<R: Resource + Any>(&self) -> Option<PrwReadLock<R>> {
        self.resources.get::<PrwLock<R>>().map(|lock| lock.read())
    }

    /// Attempts to get mutable access to a resource.
    pub fn get_mut<R: Resource + Any>(&self) -> Option<PrwWriteLock<R>> {
        self.resources.get::<PrwLock<R>>().map(|lock| lock.write())
    }
}
