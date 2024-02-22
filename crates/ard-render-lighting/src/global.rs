use ard_ecs::prelude::*;
use ard_math::{Vec3, Vec4, Vec4Swizzles};
use ard_render_si::types::GpuGlobalLighting;

#[derive(Resource, Clone)]
pub struct GlobalLighting {
    ambient_color_intensity: Vec4,
    sun_color_intensity: Vec4,
    sun_direction: Vec4,
}

impl Default for GlobalLighting {
    fn default() -> Self {
        Self {
            ambient_color_intensity: Vec4::new(0.95, 1.0, 1.0, 0.05 * 4.0),
            sun_color_intensity: Vec4::new(1.0, 0.98, 0.92, 3.0 * 8.0),
            sun_direction: Vec4::new(1.0, -1.0, 1.0, 0.0).normalize(),
        }
    }
}

impl GlobalLighting {
    pub fn to_gpu(&self) -> GpuGlobalLighting {
        GpuGlobalLighting {
            ambient_color_intensity: self.ambient_color_intensity,
            sun_color_intensity: self.sun_color_intensity,
            sun_direction: self.sun_direction,
        }
    }

    #[inline]
    pub fn sun_direction(&self) -> Vec3 {
        self.sun_direction.xyz().normalize()
    }

    #[inline]
    pub fn set_ambient_color(&mut self, color: Vec3) {
        self.ambient_color_intensity = Vec4::from((color, self.ambient_color_intensity.w));
    }

    #[inline]
    pub fn set_ambient_intensity(&mut self, intensity: f32) {
        self.ambient_color_intensity.w = intensity;
    }

    #[inline]
    pub fn set_sun_color(&mut self, color: Vec3) {
        self.sun_color_intensity = Vec4::from((color, self.sun_color_intensity.w));
    }

    #[inline]
    pub fn set_sun_intensity(&mut self, intensity: f32) {
        self.sun_color_intensity.w = intensity;
    }

    #[inline]
    pub fn set_sun_direction(&mut self, dir: Vec3) {
        self.sun_direction = Vec4::from((
            dir.try_normalize().unwrap_or(Vec3::new(0.0, -1.0, 0.0)),
            0.0,
        ));
    }
}
