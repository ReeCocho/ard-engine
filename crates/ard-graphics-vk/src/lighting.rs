use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use bytemuck::{Pod, Zeroable};

use crate::{
    alloc::UniformBuffer,
    camera::{CameraUbo, GraphicsContext},
    VkBackend,
};

#[derive(Resource)]
pub struct Lighting {
    /// Contains the data to be sent to the actual UBO.
    pub(crate) ubo_data: LightingUbo,
    /// Shared UBO containing lighting information.
    pub(crate) ubo: UniformBuffer,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub(crate) struct RawPointLight {
    /// Color is `(x, y, z)` and `w` is intensity.
    pub color_intensity: Vec4,
    /// Position is `(x, y, z)` and `w` is range.
    pub position_range: Vec4,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub(crate) struct LightingUbo {
    pub sun_vp: Mat4,
    pub ambient: Vec4,
    pub sun_color_intensity: Vec4,
    pub sun_direction: Vec4,
    pub shadow_bias_min: f32,
    pub shadow_bias_max: f32,
}

impl Default for LightingUbo {
    fn default() -> Self {
        Self {
            sun_vp: Mat4::IDENTITY,
            ambient: Vec4::new(0.03, 0.03, 0.03, 1.0),
            sun_color_intensity: Vec4::new(1.0, 1.0, 1.0, 0.0),
            sun_direction: Vec4::new(0.0, -1.0, 0.0, 1.0),
            shadow_bias_min: 0.0001,
            shadow_bias_max: 0.001,
        }
    }
}

impl Lighting {
    pub(crate) unsafe fn new(ctx: &GraphicsContext) -> Self {
        let lubo = LightingUbo {
            sun_direction: Vec4::from((
                (Vec3::ZERO - Vec3::new(32.0, 32.0, 32.0)).normalize(),
                1.0,
            )),
            ..Default::default()
        };

        let ubo = UniformBuffer::new(ctx, lubo);

        Self {
            ubo,
            ubo_data: lubo,
        }
    }

    /// Updates the lighting UBO for a particular frame.
    ///
    /// Takes in a `CameraUbo` to compute an optimal sun vp matrix.
    ///
    /// Returns a `CameraUbo` representing the light camera.
    #[inline]
    pub(crate) unsafe fn update_ubo(&mut self, frame: usize, camera: &CameraUbo) -> CameraUbo {
        // Compute the corners of the view frustum in world space.
        let mut corners = [Vec4::ZERO; 8];
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let pt = camera.vp_inv
                        * Vec4::new(2.0 * x as f32 - 1.0, 2.0 * y as f32 - 1.0, z as f32, 1.0);

                    corners[(x * 4) + (y * 2) + z] = pt / pt.w;
                }
            }
        }

        // Compute the center of the frustum by averaging all the points.
        let mut center = Vec3::ZERO;
        for corner in &corners {
            center += corner.xyz();
        }
        center /= 8.0;

        // Compute the view matrix for the light
        let view = Mat4::look_at_lh(center, center + self.ubo_data.sun_direction.xyz(), Vec3::Y);

        // We need to compute the left/right/top/bottom/near/far values for the ortho-projection
        // for the sun. To do this, we convert the corners of the view frustum in world space to
        // light space, and then take the min and max there.
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        for corner in &corners {
            let pt = view * (*corner);
            min = min.min(pt.xyz());
            max = max.max(pt.xyz());
        }

        let proj = Mat4::orthographic_lh(min.x, max.x, min.y, max.y, min.z, max.z);

        // Compute the vp matrix
        self.ubo_data.sun_vp = proj * view;

        // Write to the UBO
        self.ubo.write(self.ubo_data, frame);

        CameraUbo {
            view,
            projection: proj,
            vp: self.ubo_data.sun_vp,
            view_inv: view.inverse(),
            projection_inv: proj.inverse(),
            vp_inv: self.ubo_data.sun_vp.inverse(),
            frustum: Frustum::from(self.ubo_data.sun_vp),
            position: Vec4::from((center - (min.z * self.ubo_data.sun_direction.xyz()), 1.0)),
            cluster_scale_bias: Vec2::ZERO,
            fov: 1.0,
            near_clip: 1.0,
            far_clip: 1.0,
        }
    }
}

impl LightingApi<VkBackend> for Lighting {}

unsafe impl Pod for RawPointLight {}
unsafe impl Zeroable for RawPointLight {}

unsafe impl Pod for LightingUbo {}
unsafe impl Zeroable for LightingUbo {}
