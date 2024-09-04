use ard_ecs::component::Component;
use ard_math::{Vec3, Vec4};
use ard_render_si::types::GpuLight;

pub mod clustering;
pub mod global;
pub mod lights;
pub mod proc_skybox;
pub mod reflections;
pub mod shadows;

#[derive(Debug, Component, Copy, Clone)]
pub enum Light {
    Point {
        color: Vec3,
        range: f32,
        intensity: f32,
    },
    Spot {
        color: Vec3,
        range: f32,
        intensity: f32,
        half_angle: f32,
    },
}

impl Light {
    #[inline]
    pub fn to_gpu_light(self, position: Vec3, direction: Vec3) -> GpuLight {
        match self {
            Light::Point {
                color,
                range,
                intensity,
            } => GpuLight {
                color_intensity: Vec4::new(color.x, color.y, color.z, intensity),
                position_range: Vec4::new(position.x, position.y, position.z, range),
                direction_angle: Vec4::NEG_ONE,
            },
            Light::Spot {
                color,
                range,
                intensity,
                half_angle,
            } => GpuLight {
                color_intensity: Vec4::new(color.x, color.y, color.z, intensity),
                position_range: Vec4::new(position.x, position.y, position.z, range),
                direction_angle: Vec4::new(direction.x, direction.y, direction.z, half_angle),
            },
        }
    }
}
