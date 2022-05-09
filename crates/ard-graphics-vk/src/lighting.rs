use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::Vec4;
use bytemuck::{Pod, Zeroable};

use crate::VkBackend;

#[derive(Resource)]
pub struct Lighting {}

#[derive(Debug, Copy, Clone)]
pub(crate) struct RawPointLight {
    /// Color is `(x, y, z)` and `w` is intensity.
    pub color_intensity: Vec4,
    /// Position is `(x, y, z)` and `w` is range.
    pub position_range: Vec4,
}

impl LightingApi<VkBackend> for Lighting {}

unsafe impl Pod for RawPointLight {}

unsafe impl Zeroable for RawPointLight {}
