use ard_ecs::prelude::*;
pub use rapier3d::dynamics::RigidBodyType;
use serde::{Deserialize, Serialize};

use crate::engine::PhysicsEngine;

#[derive(Component, Clone, Copy, Serialize, Deserialize)]
pub struct RigidBody {
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub body_type: RigidBodyType,
    pub can_sleep: bool,
    pub ccd_enabled: bool,
    pub soft_ccd_prediction: f32,
}

#[derive(Component)]
pub struct RigidBodyHandle {
    handle: rapier3d::dynamics::RigidBodyHandle,
    engine: PhysicsEngine,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            body_type: RigidBodyType::Dynamic,
            can_sleep: true,
            ccd_enabled: false,
            soft_ccd_prediction: 0.0,
        }
    }
}

impl RigidBodyHandle {
    pub fn new(handle: rapier3d::dynamics::RigidBodyHandle, engine: PhysicsEngine) -> Self {
        Self { handle, engine }
    }

    #[inline(always)]
    pub fn handle(&self) -> rapier3d::dynamics::RigidBodyHandle {
        self.handle
    }
}

impl Drop for RigidBodyHandle {
    fn drop(&mut self) {
        let mut engine = self.engine.0.lock().unwrap();
        let engine = &mut *engine;
        engine.rigid_bodies.remove(
            self.handle,
            &mut engine.island_manager,
            &mut engine.colliders,
            &mut engine.impulse_joints,
            &mut engine.multibody_joints,
            false,
        );
    }
}
