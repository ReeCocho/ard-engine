use ard_ecs::prelude::*;
use ard_math::Vec3;
use rapier3d::geometry::{Ball, Capsule, Cone, Cuboid, Cylinder, SharedShape};
pub use rapier3d::prelude::CoefficientCombineRule;
use serde::{Deserialize, Serialize};

use crate::engine::PhysicsEngine;

#[derive(Component, Clone, Copy, Serialize, Deserialize)]
pub struct Collider {
    pub shape: Shape,
    pub friction: f32,
    pub friction_combine_rule: CoefficientCombineRule,
    pub restitution: f32,
    pub restitution_combine_rule: CoefficientCombineRule,
    pub mass: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Shape {
    Ball { radius: f32 },
    Capsule { radius: f32, height: f32 },
    Box { half_extents: Vec3 },
    Cylinder { height: f32, radius: f32 },
    Cone { height: f32, radius: f32 },
}

#[derive(Component)]
pub struct ColliderHandle {
    handle: rapier3d::geometry::ColliderHandle,
    engine: PhysicsEngine,
}

impl ColliderHandle {
    pub fn new(handle: rapier3d::geometry::ColliderHandle, engine: PhysicsEngine) -> Self {
        Self { handle, engine }
    }

    #[inline(always)]
    pub fn handle(&self) -> rapier3d::geometry::ColliderHandle {
        self.handle
    }
}

impl Drop for ColliderHandle {
    fn drop(&mut self) {
        let mut engine = self.engine.0.lock().unwrap();
        let engine = &mut *engine;
        engine.colliders.remove(
            self.handle,
            &mut engine.island_manager,
            &mut engine.rigid_bodies,
            false,
        );
    }
}

impl From<Shape> for SharedShape {
    fn from(value: Shape) -> Self {
        use std::sync::Arc;
        SharedShape(match value {
            Shape::Ball { radius } => Arc::new(Ball::new(radius)),
            Shape::Capsule { radius, height } => Arc::new(Capsule::new_y(height * 0.5, radius)),
            Shape::Box { half_extents } => Arc::new(Cuboid::new(half_extents.into())),
            Shape::Cylinder { height, radius } => Arc::new(Cylinder::new(height * 0.5, radius)),
            Shape::Cone { height, radius } => Arc::new(Cone::new(height * 0.5, radius)),
        })
    }
}
