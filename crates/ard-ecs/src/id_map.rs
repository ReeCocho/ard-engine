use std::{
    any::TypeId,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{BuildHasherDefault, Hasher},
};

/// Hash map that uses a no-op hasher for `TypeId`s.
pub(crate) type TypeIdMap<V> = HashMap<TypeId, V, BuildHasherDefault<FastIntHasher>>;

#[derive(Default)]
pub(crate) struct FastIntHasher {
    hash: u64,
}

impl Hasher for FastIntHasher {
    #[inline]
    fn write_u64(&mut self, n: u64) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n;
    }

    #[inline]
    fn write_u128(&mut self, n: u128) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n as u64;
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.hash, 0);

        let mut hasher = DefaultHasher::default();
        hasher.write(bytes);
        self.hash = hasher.finish();
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}
