use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_math::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    destroy::{Destroy, Destroyer},
    scene::MappedEntity,
    serialization::SerializableComponent,
};

pub const INLINE_CHILDREN: usize = 8;

/// Describes the position, rotation, and scale of an entity in local space.
#[derive(Debug, Serialize, Deserialize, Component, Copy, Clone)]
pub struct Transform {
    position: Vec3A,
    rotation: Quat,
    scale: Vec3A,
}

/// Marks an entity with a transform as being dynamic. This allows you to modify the transform.
#[derive(Debug, Default, Component, Copy, Clone)]
pub struct DynamicTransformMark(bool);

/// The global space transformation matrix of an entity.
#[derive(Debug, Default, Component, Copy, Clone)]
pub struct GlobalTransform(pub Mat4);

#[derive(Debug, Default, Component)]
pub struct Children(pub(crate) SmallVec<[Entity; INLINE_CHILDREN]>);

#[derive(Debug, Default, Component, Copy, Clone)]
pub struct Parent(pub(crate) Option<Entity>);

#[derive(Debug, Component, Copy, Clone)]
pub struct PrevParent(pub(crate) Option<Entity>);

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

impl SerializableComponent for Parent {
    type Descriptor = Option<MappedEntity>;

    fn save(
        &self,
        entities: &crate::scene::EntityMap,
        _: &ard_assets::manager::Assets,
    ) -> Self::Descriptor {
        self.0.map(|entity| entities.to_map(entity))
    }

    fn load(
        descriptors: Vec<Self::Descriptor>,
        entities: &crate::scene::EntityMap,
        _: &ard_assets::manager::Assets,
    ) -> Result<Vec<Self>, ()> {
        let mut parents = Vec::with_capacity(descriptors.len());
        for map in descriptors {
            parents.push(Parent(map.map(|entity| entities.from_map(entity))));
        }
        Ok(parents)
    }
}

impl SerializableComponent for Children {
    type Descriptor = SmallVec<[MappedEntity; INLINE_CHILDREN]>;

    fn save(
        &self,
        entities: &crate::scene::EntityMap,
        _: &ard_assets::manager::Assets,
    ) -> Self::Descriptor {
        let mut descriptor =
            SmallVec::<[MappedEntity; INLINE_CHILDREN]>::with_capacity(self.0.len());
        for entity in &self.0 {
            descriptor.push(entities.to_map(*entity));
        }
        descriptor
    }

    fn load(
        descriptors: Vec<Self::Descriptor>,
        entities: &crate::scene::EntityMap,
        _: &ard_assets::manager::Assets,
    ) -> Result<Vec<Self>, ()> {
        let mut children = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            let mut unmapped =
                SmallVec::<[Entity; INLINE_CHILDREN]>::with_capacity(descriptor.len());
            for child in &descriptor {
                unmapped.push(entities.from_map(*child));
            }
            children.push(Children(unmapped));
        }
        Ok(children)
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
