use ard_ecs::prelude::*;
use ard_math::{Vec3, Vec4};
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};

use crate::shader_constants::FRAMES_IN_FLIGHT;

#[derive(Resource)]
pub struct Lighting {
    pub(crate) data: LightingUbo,
    pub(crate) ubo: Buffer,
}

#[derive(Component, Copy, Clone)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct LightingUbo {
    pub ambient_color_intensity: Vec4,
    pub sun_color_intensity: Vec4,
    pub sun_direction: Vec4,
    pub sun_size: f32,
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

unsafe impl Pod for LightingUbo {}
unsafe impl Zeroable for LightingUbo {}

impl Lighting {
    pub(crate) fn new(ctx: &Context) -> Self {
        let ubo = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<LightingUbo>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("lighting_ubo")),
            },
        )
        .unwrap();

        Self {
            ubo,
            data: LightingUbo {
                ambient_color_intensity: Vec4::new(1.0, 0.98, 0.92, 0.2),
                sun_color_intensity: Vec4::new(1.0, 0.98, 0.92, 16.0),
                sun_direction: Vec4::new(1.0, -5.0, 1.0, 0.0).normalize(),
                sun_size: 0.5,
            },
        }
    }

    #[inline]
    pub(crate) fn update_ubo(&mut self, frame: usize) {
        let mut view = self.ubo.write(frame).unwrap();
        let slice = bytemuck::cast_slice_mut::<_, LightingUbo>(view.as_mut());
        slice[0] = self.data;
    }
}
