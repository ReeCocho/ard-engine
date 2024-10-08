use ::serde::{Deserialize, Serialize};
use ard_ecs::prelude::*;
use ard_math::Mat4;
use bitflags::*;

pub mod keys;
pub mod objects;
pub mod set;

bitflags! {
    /// Flags for renderable objects.
    #[derive(Debug, Serialize, Deserialize, Component, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
    pub struct RenderFlags: u32 {
        /// The object will cast shadows if it is not transparent.
        const SHADOW_CASTER = 0b0000_0001;
    }
}

/// Model matrix from last frame. Used to compute velocities.
#[derive(Component, Deserialize, Serialize, Default, Clone, Copy)]
pub struct PrevFrameModel(pub Mat4);
