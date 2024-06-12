use std::{
    hash::{Hash, Hasher},
    num::{NonZeroU32, NonZeroU8},
};

use bytemuck::{Pod, Zeroable};
use thiserror::Error;

/// An entity is an identifier that is associated with a set of components in a world.
#[derive(Debug, Copy, Clone, Eq)]
#[repr(C)]
pub struct Entity(NonZeroU32);

#[derive(Error, Debug)]
#[error("u32 cannot be 0 when converting to entity")]
pub struct U32ToEntityError;

unsafe impl Pod for Entity {}
unsafe impl Zeroable for Entity {}

impl Default for Entity {
    #[inline]
    fn default() -> Self {
        Entity::null()
    }
}

impl Entity {
    #[inline]
    pub const fn new(id: u32, ver: NonZeroU8) -> Entity {
        let packed = (id & 0xFFFFFF) | ((ver.get() as u32) << 24);
        Entity(unsafe { NonZeroU32::new_unchecked(packed) })
    }

    /// Creates a handle to an entity that doesn't exist.
    #[inline]
    pub const fn null() -> Entity {
        Self::new(u32::MAX, unsafe { NonZeroU8::new_unchecked(u8::MAX) })
    }

    /// Determines if this entity is null or not.
    #[inline]
    pub fn is_null(&self) -> bool {
        *self == Entity::null()
    }

    #[inline]
    pub fn id(&self) -> u32 {
        self.0.get() & 0xFFFFFF
    }

    #[inline]
    pub fn ver(&self) -> u32 {
        self.0.get() >> 24
    }
}

impl PartialEq for Entity {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for Entity {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        u32::from(*self).hash(state);
    }
}

impl From<Entity> for u32 {
    #[inline]
    fn from(entity: Entity) -> Self {
        bytemuck::cast(entity)
    }
}

impl TryFrom<u32> for Entity {
    type Error = U32ToEntityError;

    #[inline(always)]
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(U32ToEntityError)
        } else {
            Ok(Entity(NonZeroU32::new(value).unwrap()))
        }
    }
}
