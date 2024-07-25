use ard_ecs::prelude::*;
use ard_math::Vec3;
use ard_physics::{collider::Shape, KinematicCharacterController};
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Clone, Copy)]
pub struct Actor {
    controller: KinematicCharacterController,
    desired_translation: Vec3,
    pub(crate) grounded: bool,
    shape: Shape,
}

impl Actor {
    #[inline(always)]
    pub fn controller(&self) -> &KinematicCharacterController {
        &self.controller
    }

    #[inline(always)]
    pub fn desired_translation(&self) -> Vec3 {
        self.desired_translation
    }

    #[inline(always)]
    pub fn grounded(&self) -> bool {
        self.grounded
    }

    #[inline(always)]
    pub fn set_desired_translation(&mut self, translation: Vec3) {
        self.desired_translation = translation;
    }

    #[inline(always)]
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
}

impl Default for Actor {
    fn default() -> Self {
        Self {
            controller: KinematicCharacterController::default(),
            desired_translation: Vec3::ZERO,
            grounded: false,
            shape: Shape::Capsule {
                radius: 0.5,
                height: 1.5,
            },
        }
    }
}
