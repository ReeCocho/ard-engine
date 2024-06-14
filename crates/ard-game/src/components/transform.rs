use ard_ecs::prelude::*;
use ard_math::*;
use ard_save_load::{entity_map::MappedEntity, LoadContext, SaveContext, SaveLoad};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub const INLINE_CHILDREN: usize = 4;

#[derive(Debug, Component, Serialize, Deserialize, Clone, Copy)]
pub struct Position(pub Vec3A);

#[derive(Debug, Component, Serialize, Deserialize, Clone, Copy)]
pub struct Rotation(pub Quat);

#[derive(Debug, Component, Serialize, Deserialize, Clone, Copy)]
pub struct Scale(pub Vec3A);

#[derive(Debug, Default, Component, Clone)]
pub struct Children(pub SmallVec<[Entity; INLINE_CHILDREN]>);

#[derive(Debug, Component, Copy, Clone)]
pub struct Parent(pub Entity);

/// Attach to an entity to update it's parent.
#[derive(Debug, Component, Copy, Clone)]
pub struct SetParent(pub Option<Entity>);

#[derive(Serialize, Deserialize)]
pub struct SavedParent(MappedEntity);

#[derive(Serialize, Deserialize)]
pub struct SavedChildren(SmallVec<[MappedEntity; INLINE_CHILDREN]>);

impl SaveLoad for Parent {
    type Intermediate = SavedParent;

    fn save(&self, ctx: &SaveContext) -> Self::Intermediate {
        SavedParent(ctx.entity_map.to_map(self.0))
    }

    fn load(ctx: &LoadContext, intermediate: Self::Intermediate) -> Self {
        Self(ctx.entity_map.from_map(intermediate.0))
    }
}

impl SaveLoad for Children {
    type Intermediate = SavedChildren;

    fn save(&self, ctx: &SaveContext) -> Self::Intermediate {
        SavedChildren(self.0.iter().map(|e| ctx.entity_map.to_map(*e)).collect())
    }

    fn load(ctx: &LoadContext, intermediate: Self::Intermediate) -> Self {
        Self(
            intermediate
                .0
                .into_iter()
                .map(|e| ctx.entity_map.from_map(e))
                .collect(),
        )
    }
}

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
