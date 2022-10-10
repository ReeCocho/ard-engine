use std::{any::TypeId, ptr::NonNull};

use crate::{
    key::TypeKey,
    prw_lock::{PrwReadLock, PrwWriteLock},
};

use super::{filter::ResourceFilter, Resource, Resources};

/// A container of resources used by a system.
pub struct Res<R: ResourceFilter> {
    resources: NonNull<Resources>,
    all: TypeKey,
    writes: TypeKey,
    _phantom: std::marker::PhantomData<R>,
}

impl<R: ResourceFilter> Res<R> {
    pub(crate) fn new(resources: &Resources) -> Self {
        Self {
            resources: unsafe { NonNull::new_unchecked(resources as *const _ as *mut _) },
            all: R::type_key(),
            writes: R::mut_type_key(),
            _phantom: Default::default(),
        }
    }

    #[inline(always)]
    pub fn get<T: Resource + 'static>(&self) -> Option<PrwReadLock<T>> {
        if !R::EVERYTHING {
            debug_assert!(self.all.contains(TypeId::of::<T>()));
        }
        unsafe { self.resources.as_ref().get::<T>() }
    }

    #[inline(always)]
    pub fn get_mut<T: Resource + 'static>(&self) -> Option<PrwWriteLock<T>> {
        if !R::EVERYTHING {
            debug_assert!(self.writes.contains(TypeId::of::<T>()));
        }
        unsafe { self.resources.as_ref().get_mut::<T>() }
    }
}
