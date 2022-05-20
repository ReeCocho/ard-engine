use crate::prelude::{Asset, Assets};
use std::{
    hash::Hash,
    num::NonZeroU32,
    sync::{atomic::Ordering, Arc},
};

/// Raw asset handle.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawHandle {
    pub id: u32,
    pub ver: NonZeroU32,
}

/// A handle to an asset in the asset manger.
pub struct Handle<T: Asset> {
    raw: RawHandle,
    escaper: Arc<HandleEscaper>,
    _phantom: std::marker::PhantomData<T>,
}

pub struct HandleEscaper {
    assets: Assets,
    id: u32,
}

unsafe impl<T: Asset> Send for Handle<T> {}
unsafe impl<T: Asset> Sync for Handle<T> {}

impl<T: Asset> Handle<T> {
    #[inline]
    pub(crate) fn new(id: u32, ver: NonZeroU32, assets: Assets) -> Self {
        let raw = RawHandle { id, ver };

        Self {
            raw,
            escaper: Arc::new(HandleEscaper { assets, id }),
            _phantom: std::marker::PhantomData::default(),
        }
    }

    /// Transmutes the handle type from one to another. This is used internally for places where
    /// the type system can't determine where two asset types are the same.
    ///
    /// # Safety
    /// This is unsafe because the types could mismatch.
    #[inline]
    pub unsafe fn transmute<U: Asset>(self) -> Handle<U> {
        Handle::<U> {
            raw: self.raw,
            escaper: self.escaper,
            _phantom: Default::default(),
        }
    }

    #[inline]
    pub fn id(&self) -> u32 {
        self.raw.id
    }

    #[inline]
    pub fn ver(&self) -> NonZeroU32 {
        self.raw.ver
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle::<T> {
            raw: self.raw.clone(),
            escaper: self.escaper.clone(),
            _phantom: Default::default(),
        }
    }
}

impl<T: Asset> Hash for Handle<T> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.raw.id);
        state.write_u32(self.raw.ver.get());
    }
}

impl<T: Asset> PartialEq for Handle<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T: Asset> Eq for Handle<T> {}

impl Drop for HandleEscaper {
    #[inline]
    fn drop(&mut self) {
        let guard = self.assets.0.assets.guard();
        let asset_data = self.assets.0.assets.get(&self.id, &guard).unwrap();

        let mut asset = asset_data.asset.write().unwrap();

        if let Some(outstanding) = asset_data.outstanding_handles.as_ref() {
            let last_handle = outstanding.fetch_sub(1, Ordering::Relaxed) == 1;
            if last_handle {
                *asset = None;
            }
        }
    }
}
