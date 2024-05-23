use ard_ecs::prelude::*;
use ard_math::*;
use smallvec::SmallVec;

pub const INLINE_CHILDREN: usize = 4;

#[derive(Debug, Component, Clone, Copy)]
pub struct Position(pub Vec3A);

#[derive(Debug, Component, Clone, Copy)]
pub struct Rotation(pub Quat);

#[derive(Debug, Component, Clone, Copy)]
pub struct Scale(pub Vec3A);

#[derive(Debug, Default, Component, Clone)]
pub struct Children(pub SmallVec<[Entity; INLINE_CHILDREN]>);

#[derive(Debug, Component, Copy, Clone)]
pub struct Parent(pub Entity);

/// Attach to an entity to update it's parent.
#[derive(Debug, Component, Copy, Clone)]
pub struct SetParent(pub Option<Entity>);

impl Default for Position {
    fn default() -> Self {
        Position(Vec3A::ZERO)
    }
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation(Quat::IDENTITY)
    }
}

impl Default for Scale {
    fn default() -> Self {
        Scale(Vec3A::ONE)
    }
}
