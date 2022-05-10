use std::ptr::NonNull;

use super::{filter::ResourceFilter, Resources};

/// A container of resources used by a system.
pub struct Res<R: ResourceFilter> {
    resources: NonNull<Resources>,
    _phantom: std::marker::PhantomData<R>,
}

impl<R: ResourceFilter> Res<R> {
    pub(crate) fn new(resources: &Resources) -> Self {
        Self {
            resources: unsafe { NonNull::new_unchecked(resources as *const _ as *mut _) },
            _phantom: Default::default(),
        }
    }

    pub fn get(self) -> R::Set {
        R::get(unsafe { self.resources.as_ref() })
    }
}
