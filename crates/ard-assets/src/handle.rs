use crate::prelude::{Asset, Assets};
use std::{hash::Hash, sync::Arc};

/// Raw asset handle.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawHandle {
    pub id: u32,
}

/// A handle to an asset in the asset manger.
pub struct Handle<T: Asset> {
    raw: RawHandle,
    escaper: Arc<HandleEscaper>,
    _phantom: std::marker::PhantomData<T>,
}

/// A handle to a generic asset. Useful for when you need to keep an asset alive but don't know
/// it's type.
pub struct AnyHandle {
    raw: RawHandle,
    escaper: Arc<HandleEscaper>,
}

pub struct HandleEscaper {
    assets: Assets,
    id: u32,
}

unsafe impl<T: Asset> Send for Handle<T> {}
unsafe impl<T: Asset> Sync for Handle<T> {}

impl<T: Asset> Handle<T> {
    #[inline]
    pub(crate) fn new(id: u32, assets: Assets) -> Self {
        let raw = RawHandle { id };

        Self {
            raw,
            escaper: Arc::new(HandleEscaper { assets, id }),
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn assets(&self) -> &Assets {
        &self.escaper.assets
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
    pub fn raw(&self) -> RawHandle {
        self.raw
    }

    #[inline]
    pub fn id(&self) -> u32 {
        self.raw.id
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle::<T> {
            raw: self.raw,
            escaper: self.escaper.clone(),
            _phantom: Default::default(),
        }
    }
}

impl Clone for AnyHandle {
    fn clone(&self) -> Self {
        AnyHandle {
            raw: self.raw,
            escaper: self.escaper.clone(),
        }
    }
}

impl<T: Asset> Hash for Handle<T> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.raw.id);
    }
}

impl<T: Asset> PartialEq for Handle<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T: Asset> Eq for Handle<T> {}

impl<T: Asset> From<Handle<T>> for AnyHandle {
    fn from(value: Handle<T>) -> Self {
        AnyHandle {
            raw: value.raw,
            escaper: value.escaper,
        }
    }
}

impl Drop for HandleEscaper {
    #[inline]
    fn drop(&mut self) {
        let asset_data = self.assets.0.assets.get(&self.id).unwrap();
        let mut asset = asset_data.asset.write().unwrap();
        if asset_data.decrement_handle_counter() {
            *asset = None;
        }
    }
}
