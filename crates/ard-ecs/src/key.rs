use std::{
    any::TypeId,
    cmp::Ordering,
    ops::{Add, AddAssign},
};

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct TypeKey {
    types: Vec<TypeId>,
}

impl TypeKey {
    /// Initialize a type key from a presorted vec of type ids.
    ///
    /// # Safety
    /// It is up to the caller to ensure that the vec is already sorted.
    #[inline]
    pub unsafe fn pre_sorted(types: Vec<TypeId>) -> TypeKey {
        TypeKey { types }
    }

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

    /// Indicates that this type key and the other don't contain any types in common.
    #[inline]
    pub fn disjoint(&self, other: &TypeKey) -> bool {
        let mut self_idx = 0;
        let mut other_idx = 0;
        while self_idx < self.types.len() && other_idx < other.types.len() {
            let self_ty = self.types[self_idx];
            let other_ty = other.types[other_idx];

            match self_ty.cmp(&other_ty) {
                Ordering::Equal => return false,
                Ordering::Less => self_idx += 1,
                Ordering::Greater => other_idx += 1,
            }
        }
        true
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
        let mut merged = Vec::with_capacity(self.types.len() + rhs.types.len());
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

        merged.extend(if i1 < end1 {
            &self.types[i1..]
        } else {
            &rhs.types[i2..]
        });

        self.types = merged;
    }
}
