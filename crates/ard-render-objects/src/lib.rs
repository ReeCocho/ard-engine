use std::ops::Mul;

use ::serde::{Deserialize, Serialize};
use ard_ecs::prelude::Component;
use ard_math::{Mat3, Mat4, Quat, Vec3, Vec3A, Vec4Swizzles};
use bitflags::*;

pub mod keys;
pub mod objects;
pub mod set;

/// Model matrix to describe the transformation of a renderable object.
#[derive(Component, Deserialize, Serialize, Default, Clone, Copy)]
pub struct Model(pub Mat4);

bitflags! {
    /// Flags for renderable objects.
    #[derive(Debug, Serialize, Deserialize, Component, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
    pub struct RenderFlags: u32 {
        /// The object will cast shadows if it is not transparent.
        const SHADOW_CASTER = 0b0000_0001;
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
        debug_assert!(det != 0.0);
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
