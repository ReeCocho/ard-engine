use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{BuildHasherDefault, Hasher},
};

pub type FIHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FastIntHasher>>;

#[derive(Default)]
pub struct FastIntHasher {
    hash: u64,
}

impl Hasher for FastIntHasher {
    #[inline(always)]
    fn write_u64(&mut self, n: u64) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n;
    }

    #[inline(always)]
    fn write_u128(&mut self, n: u128) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n as u64;
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.hash, 0);

        let mut hasher = DefaultHasher::default();
        hasher.write(bytes);
        self.hash = hasher.finish();
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash
    }
}
