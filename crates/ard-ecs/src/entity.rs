use std::{
    hash::{Hash, Hasher},
    num::NonZeroU32,
};

use bytemuck::{Pod, Zeroable};

/// An entity is an identifier that is associated with a set of components in a world.
#[derive(Debug, Copy, Clone, Eq)]
#[repr(C)]
pub struct Entity {
    id: u32,
    ver: NonZeroU32,
}

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
    pub fn new(id: u32, ver: NonZeroU32) -> Entity {
        Entity { id, ver }
    }

    /// Creates a handle to an entity that doesn't exist.
    #[inline]
    pub const fn null() -> Entity {
        Entity {
            id: u32::MAX,
            ver: unsafe { NonZeroU32::new_unchecked(u32::MAX) },
        }
    }

    /// Determines if this entity is null or not.
    #[inline]
    pub fn is_null(&self) -> bool {
        *self == Entity::null()
    }

    #[inline]
    pub fn id(&self) -> u32 {
        self.id
    }

    #[inline]
    pub fn ver(&self) -> u32 {
        self.ver.get()
    }
}

impl PartialEq for Entity {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.ver == other.ver
    }
}

impl Hash for Entity {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        u64::from(*self).hash(state);
    }
}

impl From<Entity> for u64 {
    #[inline]
    fn from(entity: Entity) -> Self {
        bytemuck::cast(entity)
    }
}
