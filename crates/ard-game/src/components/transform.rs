use ard_ecs::prelude::*;
use ard_game_derive::SaveLoad;
use ard_math::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::serialization::SaveLoad;

pub const INLINE_CHILDREN: usize = 8;

/// Describes the position, rotation, and scale of an entity in local space.
///
/// # Note
/// You should not modify the fields of this component directly. You should use the `get` and set`
/// methods.
#[derive(Debug, SaveLoad, Component, Copy, Clone)]
pub struct Transform {
    pub position: Vec3A,
    pub rotation: Quat,
    pub scale: Vec3A,
}

/// Marks an entity with a transform as being dynamic. This allows you to modify the transform.
#[derive(Debug, Default, Component, Copy, Clone)]
pub struct DynamicTransformMark(bool);

#[derive(Debug, SaveLoad, Clone, Default, Component)]
pub struct Children(pub SmallVec<[Entity; INLINE_CHILDREN]>);

/// # Note
/// You should not modify this field directly. Instead, you should use the `get` and `set` methods.
#[derive(Debug, SaveLoad, Default, Component, Copy, Clone)]
pub struct Parent(pub Option<Entity>);

#[derive(Debug, Component, Copy, Clone)]
pub struct PrevParent(pub Option<Entity>);

impl Parent {
    #[inline]
    pub fn get(&self) -> Option<Entity> {
        self.0
    }

    #[inline]
    pub fn set(&mut self, self_entity: Entity, new_parent: Option<Entity>, commands: &Commands) {
        let entity = std::mem::replace(&mut self.0, new_parent);
        commands
            .entities
            .add_component(self_entity, PrevParent(entity))
    }
}

impl Transform {
    #[inline]
    pub fn position(&self) -> Vec3 {
        self.position.into()
    }

    #[inline]
    pub fn rotation(&self) -> Quat {
        self.rotation
    }

    #[inline]
    pub fn scale(&self) -> Vec3 {
        self.scale.into()
    }

    #[inline]
    pub fn local_transform(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.scale.into(),
            self.rotation,
            self.position.into(),
        )
    }

    #[inline]
    pub fn set_position(&mut self, position: Vec3, mark: &mut DynamicTransformMark) {
        self.position = position.into();
        mark.0 = true;
    }

    #[inline]
    pub fn set_rotation(&mut self, rotation: Quat, mark: &mut DynamicTransformMark) {
        self.rotation = rotation;
        mark.0 = true;
    }

    #[inline]
    pub fn set_scale(&mut self, scale: Vec3, mark: &mut DynamicTransformMark) {
        self.scale = scale.into();
        mark.0 = true;
    }
}

impl Default for Transform {
    #[inline]
    fn default() -> Self {
        Transform {
            position: Vec3::ZERO.into(),
            rotation: Quat::default(),
            scale: Vec3::ONE.into(),
        }
    }
}
