pub mod system;

use ard_core::{app::AppBuilder, plugin::Plugin, prelude::Destroy};
use ard_ecs::{prelude::*, system::data::SystemData};
use ard_math::*;
use ard_save_load::{entity_map::MappedEntity, LoadContext, SaveContext, SaveLoad};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::ops::Mul;
use system::{ModelUpdateSystem, TransformHierarchyUpdate};

pub const INLINE_CHILDREN: usize = 4;

pub struct TransformPlugin;

#[derive(Component, Deserialize, Serialize, Default, Clone, Copy)]
pub struct Model(pub Mat4);

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

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct SavedParent(MappedEntity);

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedChildren(Vec<MappedEntity>);

/// Attach to an entity to update it's parent.
#[derive(Debug, Component, Copy, Clone)]
pub struct SetParent {
    pub new_parent: Option<Entity>,
    /// Index in the parent's child list to insert the entity at. Saturates when OOB.
    pub index: usize,
}

/// Recursively destroys an entity and all of it's children.
/// NOTE: The provided `queries` must support `Read<Children>`.
pub fn destroy_entity(
    entity: Entity,
    commands: &EntityCommands,
    queries: &Queries<impl SystemData>,
) {
    let mut to_destroy = vec![entity];
    let mut i = 0;

    while i != to_destroy.len() {
        let cur_entity = to_destroy[i];
        i += 1;

        // Mark for deletion
        commands.add_component(cur_entity, Destroy);

        // Append children
        let children = match queries.get::<Read<Children>>(cur_entity) {
            Some(children) => children,
            None => continue,
        };
        to_destroy.extend_from_slice(&children.0);
    }
}

impl Plugin for TransformPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(TransformHierarchyUpdate::default());
        app.add_system(ModelUpdateSystem::default());
    }
}

impl Model {
    #[inline(always)]
    pub fn position(&self) -> Vec3A {
        self.0.col(3).xyz().into()
    }

    #[inline(always)]
    pub fn scale(&self) -> Vec3A {
        let det = self.0.determinant();
        Vec3A::new(
            self.0.col(0).length() * det.signum(),
            self.0.col(1).length(),
            self.0.col(2).length(),
        )
    }

    #[inline(always)]
    pub fn rotation(&self) -> Quat {
        let inv_scale = self.scale().recip();
        Quat::from_mat3(&Mat3::from_cols(
            self.0.col(0).mul(inv_scale.x).xyz(),
            self.0.col(1).mul(inv_scale.y).xyz(),
            self.0.col(2).mul(inv_scale.z).xyz(),
        ))
    }

    #[inline(always)]
    pub fn right(&self) -> Vec3 {
        self.0.col(0).xyz()
    }

    #[inline(always)]
    pub fn up(&self) -> Vec3 {
        self.0.col(1).xyz()
    }

    #[inline(always)]
    pub fn forward(&self) -> Vec3 {
        self.0.col(2).xyz()
    }
}

impl Children {
    pub fn index_of(&self, target: Entity) -> Option<usize> {
        let mut index = None;
        for (i, entity) in self.0.iter().enumerate() {
            if *entity == target {
                index = Some(i);
                break;
            }
        }
        index
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

impl SaveLoad for Parent {
    type Intermediate = SavedParent;

    fn save(&self, ctx: &mut SaveContext) -> Self::Intermediate {
        SavedParent(ctx.entity_map.to_map(self.0))
    }

    fn load(ctx: &mut LoadContext, intermediate: Self::Intermediate) -> Self {
        Self(ctx.entity_map.from_map(intermediate.0))
    }
}

impl SaveLoad for Children {
    type Intermediate = SavedChildren;

    fn save(&self, ctx: &mut SaveContext) -> Self::Intermediate {
        SavedChildren(self.0.iter().map(|e| ctx.entity_map.to_map(*e)).collect())
    }

    fn load(ctx: &mut LoadContext, intermediate: Self::Intermediate) -> Self {
        Self(
            intermediate
                .0
                .into_iter()
                .map(|e| ctx.entity_map.from_map(e))
                .collect(),
        )
    }
}
