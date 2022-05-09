use std::num::NonZeroU32;

/// An entity is an identifier that is associated with a set of components in a world.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity {
    id: u32,
    ver: NonZeroU32,
}

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
