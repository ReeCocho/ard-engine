use ard_ecs::prelude::*;
use ard_math::{Vec3, Vec4, Vec4Swizzles};
use ard_render_si::types::GpuGlobalLighting;

use crate::shadows::ShadowCascadeSettings;

#[derive(Resource, Clone)]
pub struct GlobalLighting {
    ambient_color_intensity: Vec4,
    sun_color_intensity: Vec4,
    sun_direction: Vec4,
    cascades: Vec<ShadowCascadeSettings>,
}

impl Default for GlobalLighting {
    fn default() -> Self {
        Self {
            ambient_color_intensity: Vec4::new(1.0, 1.0, 1.0, 0.2),
            sun_color_intensity: Vec4::new(1.0, 0.98, 0.92, 32.0),
            sun_direction: Vec4::new(1.0, -1.0, 1.0, 0.0).normalize(),
            cascades: vec![
                ShadowCascadeSettings {
                    min_depth_bias: 0.0,
                    max_depth_bias: 0.005,
                    normal_bias: 0.1,
                    filter_size: 2.0,
                    resolution: 4096,
                    end_distance: 15.0,
                },
                ShadowCascadeSettings {
                    min_depth_bias: 0.005,
                    max_depth_bias: 0.05,
                    normal_bias: 0.1,
                    filter_size: 2.0,
                    resolution: 4096,
                    end_distance: 40.0,
                },
                ShadowCascadeSettings {
                    min_depth_bias: 0.005,
                    max_depth_bias: 0.05,
                    normal_bias: 0.1,
                    filter_size: 2.0,
                    resolution: 4096,
                    end_distance: 100.0,
                },
                ShadowCascadeSettings {
                    min_depth_bias: 0.005,
                    max_depth_bias: 0.05,
                    normal_bias: 0.25,
                    filter_size: 2.0,
                    resolution: 4096,
                    end_distance: 300.0,
                },
            ],
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
    pub fn ambient_color(&self) -> Vec3 {
        self.ambient_color_intensity.xyz()
    }

    #[inline]
    pub fn ambient_intensity(&self) -> f32 {
        self.ambient_color_intensity.w
    }

    #[inline]
    pub fn sun_color(&self) -> Vec3 {
        self.sun_color_intensity.xyz()
    }

    #[inline]
    pub fn sun_intensity(&self) -> f32 {
        self.sun_color_intensity.w
    }

    #[inline]
    pub fn sun_direction(&self) -> Vec3 {
        self.sun_direction.xyz().normalize()
    }

    #[inline]
    pub fn shadow_cascades(&self) -> &[ShadowCascadeSettings] {
        &self.cascades
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

    #[inline]
    pub fn set_shadow_cascade_settings(&mut self, cascades: &[ShadowCascadeSettings]) {
        self.cascades = cascades.into();
    }
}
