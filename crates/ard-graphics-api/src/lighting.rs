use ard_ecs::prelude::*;
use ard_math::Vec3;
use bytemuck::{Pod, Zeroable};

use crate::Backend;

pub trait LightingApi<B: Backend>: Resource + Send + Sync {}

/// A point light attached to an entity.
#[derive(Component, Clone, Copy)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub radius: f32,
}

unsafe impl Pod for PointLight {}
unsafe impl Zeroable for PointLight {}
