use crate::{
    prw_lock::{PrwReadLock, PrwWriteLock},
    resource::{Resource, Resources},
};

/// Represents a way to access a resource.
pub trait ResourceAccess {
    /// Resource type being accessed.
    type Resource: Resource;

    /// Type of lock used by the resource.
    type Lock;

    /// Indicates the resource needs mutable access.
    const MUT_ACCESS: bool;

    /// Gets the lock for the resource given a resource container.
    ///
    /// Returns `None` if the resource isn't in the container.
    ///
    /// # Panics
    /// Panics if the XOR borrowing rules are broken for the resource.
    fn get_lock(resources: &Resources) -> Option<Self::Lock>;
}

impl<R: Resource + 'static> ResourceAccess for &'static R {
    type Resource = R;
    type Lock = PrwReadLock<R>;
    const MUT_ACCESS: bool = false;

    fn get_lock(resources: &Resources) -> Option<Self::Lock> {
        resources.get::<R>()
    }
}

impl<R: Resource + 'static> ResourceAccess for &'static mut R {
    type Resource = R;
    type Lock = PrwWriteLock<R>;
    const MUT_ACCESS: bool = true;

    fn get_lock(resources: &Resources) -> Option<Self::Lock> {
        resources.get_mut::<R>()
    }
}
