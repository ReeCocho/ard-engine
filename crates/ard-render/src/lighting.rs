use ard_ecs::prelude::*;
use ard_math::{Vec3, Vec4};
use bytemuck::{Pod, Zeroable};

#[derive(Component, Copy, Clone)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) union RawLight {
    pub point: RawPointLight,
    pub spot: RawSpotLight,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct RawPointLight {
    pub color_intensity: Vec4,
    pub position_range: Vec4,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct RawSpotLight {
    pub color_intensity: Vec4,
    pub position_range: Vec4,
    pub direction_radius: Vec4,
}

unsafe impl Pod for RawLight {}
unsafe impl Zeroable for RawLight {}

unsafe impl Pod for RawPointLight {}
unsafe impl Zeroable for RawPointLight {}

unsafe impl Pod for RawSpotLight {}
unsafe impl Zeroable for RawSpotLight {}
