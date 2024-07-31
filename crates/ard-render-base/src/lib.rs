use std::time::Duration;

use ard_ecs::{component::Component, event::Event};
use serde::{Deserialize, Serialize};

pub mod resource;
pub mod shader_variant;

/// Describes what type of rendering is required for a particular entity.
#[derive(Component, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderingMode {
    /// The entity is fully opaque.
    Opaque = 0,
    /// The entity is opaque, but might have holes in the geometry from alpha masking.
    AlphaCutout = 1,
    /// The entity is transparent,
    Transparent = 2,
}

/// Frame index.
#[derive(Debug, Copy, Clone)]
pub struct Frame(usize);

impl From<usize> for Frame {
    fn from(value: usize) -> Self {
        Frame(value)
    }
}

impl From<Frame> for usize {
    fn from(value: Frame) -> Self {
        value.0
    }
}
/// Event indicating rendering is about to occur.
#[derive(Debug, Event, Copy, Clone)]
pub struct PreRender(pub Duration);

pub const FRAMES_IN_FLIGHT: usize = 2;
