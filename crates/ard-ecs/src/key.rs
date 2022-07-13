use std::{
    any::TypeId,
    ops::{Add, AddAssign},
};

use smallvec::SmallVec;

const INLINE_KEYS: usize = 8;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct TypeKey {
    types: SmallVec<[TypeId; INLINE_KEYS]>,
}

impl TypeKey {
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<TypeId> {
        self.types.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    #[inline]
    /// Returns true if the key contained the type removed.
    pub fn remove<T: 'static>(&mut self) -> bool {
        self.remove_by_id(TypeId::of::<T>())
    }

    /// Returns true if the key contained the type removed.
    #[inline]
    pub fn remove_by_id(&mut self, id: TypeId) -> bool {
        if let Ok(pos) = self.types.binary_search(&id) {
            self.types.remove(pos);
            true
        } else {
            false
        }
    }

    /// Returns true if the type was already present.
    #[inline]
    pub fn add<T: 'static>(&mut self) -> bool {
        self.add_by_id(TypeId::of::<T>())
    }

    /// Returns true if the type was already present.
    #[inline]
    pub fn add_by_id(&mut self, id: TypeId) -> bool {
        if let Err(pos) = self.types.binary_search(&id) {
            self.types.insert(pos, id);
            false
        } else {
            true
        }
    }

    /// Indicates that the two type keys contain none of the same elements. i.e., their
    /// intersection is the empty set.
    #[inline]
    pub fn none_of(&self, other: &TypeKey) -> bool {
        if self.is_empty() || other.is_empty() {
            return true;
        }

        let mut self_i = 0;
        let mut other_i = 0;

        loop {
            let self_id = self.types[self_i];
            let other_id = other.types[other_i];

            match self_id.cmp(&other_id) {
                std::cmp::Ordering::Less => {
                    self_i += 1;

                    if self_i == self.types.len() {
                        return true;
                    }
                }
                std::cmp::Ordering::Equal => {
                    return false;
                }
                std::cmp::Ordering::Greater => {
                    other_i += 1;

                    if other_i == other.types.len() {
                        return true;
                    }
                }
            }
        }
    }

    /// Indicates that this type key contains types that are all found in the other key.
    #[inline]
    pub fn subset_of(&self, other: &TypeKey) -> bool {
        if self.types.is_empty() {
            return true;
        }

        let mut i = 0;
        for ty in &other.types {
            if *ty == self.types[i] {
                i += 1;
                if i == self.types.len() {
                    return true;
                }
            }
        }

        false
    }
}

impl Add for TypeKey {
    type Output = TypeKey;

    #[inline]
    fn add(mut self, rhs: Self) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign for TypeKey {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        let mut merged = SmallVec::with_capacity(self.types.len() + rhs.types.len());
        let (end1, end2) = (self.types.len(), rhs.types.len());
        let (mut i1, mut i2) = (0usize, 0usize);
        while i1 < end1 && i2 < end2 {
            let (x1, x2) = (self.types[i1], rhs.types[i2]);
            merged.push(if x1 < x2 {
                i1 += 1;
                x1
            } else {
                i2 += 1;
                x2
            });
        }

        if i1 < end1 {
            for i in &self.types[i1..self.types.len()] {
                merged.push(*i);
            }
        } else {
            for i in &rhs.types[i2..rhs.types.len()] {
                merged.push(*i);
            }
        }

        // merged.extend(if i1 < end1 {
        //     self.types[i1..].iter()
        // } else {
        //     rhs.types[i2..].iter()
        // });

        self.types = merged;
    }
}
