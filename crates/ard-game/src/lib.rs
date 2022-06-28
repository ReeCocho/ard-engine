use ard_ecs::prelude::*;
use ard_math::*;

#[derive(Debug, Component, Copy, Clone)]
pub struct Transform {
    position: Vec3A,
    rotation: Quat,
    scale: Vec3A,
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
}
