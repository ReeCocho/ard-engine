use std::ops::DerefMut;

use ard_math::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use ard_pal::prelude::*;
use ard_render_camera::Camera;
use ard_render_si::{consts::*, types::*};
use ard_transform::Model;

pub struct SunShadowsUbo {
    ubo: Buffer,
    cameras: [GpuCamera; MAX_SHADOW_CASCADES],
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowCascadeSettings {
    pub min_depth_bias: f32,
    pub max_depth_bias: f32,
    pub normal_bias: f32,
    pub filter_size: f32,
    pub resolution: u32,
    pub end_distance: f32,
}

impl SunShadowsUbo {
    pub fn new(ctx: &Context) -> Self {
        let mut res = Self {
            ubo: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: std::mem::size_of::<GpuSunShadows>() as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::UNIFORM_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Concurrent,
                    debug_name: Some("sun_shadows_ubo".into()),
                },
            )
            .unwrap(),
            cameras: std::array::from_fn(|_| GpuCamera {
                view: Mat4::IDENTITY,
                projection: Mat4::IDENTITY,
                vp: Mat4::IDENTITY,
                view_inv: Mat4::IDENTITY,
                projection_inv: Mat4::IDENTITY,
                vp_inv: Mat4::IDENTITY,
                frustum: GpuFrustum {
                    planes: [Vec4::ZERO; 6],
                },
                position: Vec4::ZERO,
                forward: Vec4::ZERO,
                aspect_ratio: 1.0,
                near_clip: 1.0,
                far_clip: 1.0,
                cluster_scale_bias: Vec2::ONE,
            }),
        };

        // Write in shadow kernel
        let mut buff_view = res.ubo.write(0).unwrap();
        let ubo = &mut bytemuck::cast_slice_mut::<_, GpuSunShadows>(buff_view.deref_mut())[0];
        ubo.kernel = SHADOW_KERNEL;

        std::mem::drop(buff_view);

        res
    }

    #[inline(always)]
    pub fn buffer(&self) -> &Buffer {
        &self.ubo
    }

    #[inline(always)]
    pub fn camera(&self, cascade: usize) -> Option<&GpuCamera> {
        self.cameras.get(cascade)
    }

    pub fn update(
        &mut self,
        cascades: &[ShadowCascadeSettings],
        light_dir: Vec3,
        camera: &Camera,
        camera_model: Model,
        camera_aspect: f32,
    ) {
        debug_assert!(cascades.len() <= MAX_SHADOW_CASCADES);

        let mut buff_view = self.ubo.write(0).unwrap();
        let ubo = &mut bytemuck::cast_slice_mut::<_, GpuSunShadows>(buff_view.deref_mut())[0];

        ubo.count = cascades.len() as u32;

        let mut last_cascade_end = camera.near;
        for (i, cascade) in cascades.iter().enumerate() {
            let lin_near = last_cascade_end;
            let lin_far = cascade.end_distance.max(lin_near + 0.0001);
            last_cascade_end = lin_far;

            // Bounding view matrix of the camera for the cascade
            let cam_position: Vec3 = camera_model.position().into();
            let cam_forward = camera_model.forward().try_normalize().unwrap_or(Vec3::Z);
            let cam_up = camera_model.up();
            let camera_view = Mat4::look_at_lh(
                cam_position,
                cam_position + cam_forward,
                cam_up.try_normalize().unwrap_or(Vec3::Y),
            );

            // Limited projection matrix for the camera
            let camera_proj = Mat4::perspective_lh(camera.fov, camera_aspect, lin_near, lin_far);
            let camera_vp = camera_proj * camera_view;
            let camera_vp_inv = camera_vp.inverse();

            // Determine the position of the four corners of the frustum
            let mut corners = [Vec4::ZERO; 8];
            for x in 0..2 {
                for y in 0..2 {
                    for z in 0..2 {
                        let pt = camera_vp_inv
                            * Vec4::new(2.0 * x as f32 - 1.0, 2.0 * y as f32 - 1.0, z as f32, 1.0);
                        corners[(x * 4) + (y * 2) + z] = pt / pt.w;
                    }
                }
            }

            // Find the farthest corners of the frustum
            // NOTE: O(N^2) for this. Kind of sucks. Maybe we can do better?
            let mut min = 0;
            let mut max = 0;
            let mut dist = 0.0;

            for (i, corner_i) in corners.iter().enumerate() {
                for (j, corner_j) in corners.iter().enumerate() {
                    let new_dist = (*corner_i - *corner_j).length();
                    if new_dist > dist {
                        min = i;
                        max = j;
                        dist = new_dist;
                    }
                }
            }

            let min = corners[min];
            let max = corners[max];

            // Find the radius of the frustum and the number of texels per world unit
            let radius = (max - min).length() / 2.0;
            let texels_per_unit = cascade.resolution as f32 / (radius * 2.0);
            let scaling =
                Mat4::from_scale(Vec3::new(texels_per_unit, texels_per_unit, texels_per_unit));

            // Compute the center of the frustum by averaging all the points.
            let mut center = Vec3::ZERO;
            for corner in &corners {
                center += corner.xyz();
            }
            center /= 8.0;

            // Clamp the center of the frustum to be a multiple of the texel size
            let look_at = scaling * Mat4::look_at_lh(Vec3::ZERO, light_dir, Vec3::Y);
            let look_at_inv = look_at.inverse();

            let mut new_center = Vec4::new(center.x, center.y, center.z, 1.0);
            new_center = look_at * new_center;
            new_center.x = new_center.x.floor();
            new_center.y = new_center.y.floor();
            new_center = look_at_inv * new_center;
            center = new_center.xyz();

            // Compute the view and projection matrices matrix for the light
            let eye = center;
            let view = Mat4::look_at_lh(eye, eye + light_dir, Vec3::Y);
            let proj = Mat4::orthographic_lh(-radius, radius, -radius, radius, -radius, radius);

            // Construct the frustum planes for culling. We set the back plane to 0 so that we
            // never cull objects behind the view.
            let vp = proj * view;
            let mut frustum = GpuFrustum::from(vp);
            frustum.planes[4] = Vec4::ZERO;

            self.cameras[i] = GpuCamera {
                view,
                projection: proj,
                vp,
                view_inv: view.inverse(),
                projection_inv: proj.inverse(),
                vp_inv: vp.inverse(),
                frustum,
                position: Vec4::from((eye, 1.0)),
                forward: Vec4::from((cam_forward, 0.0)),
                aspect_ratio: 1.0,
                near_clip: 1.0,
                far_clip: 1.0,
                cluster_scale_bias: Vec2::ONE,
            };

            ubo.cascades[i] = GpuShadowCascade {
                vp,
                view,
                proj,
                // TODO: Make sun size configurable
                uv_size: cascade.filter_size / (Vec2::ONE * 2.0 * radius),
                far_plane: lin_far,
                min_depth_bias: cascade.min_depth_bias,
                max_depth_bias: cascade.max_depth_bias,
                normal_bias: cascade.normal_bias,
                depth_range: 2.0 * radius,
            };
        }
    }
}

// i16 snorm values packed into u32s.
const SHADOW_KERNEL: [u32; 32] = [
    0, 1240255496, 3343704717, 958525605, 1085448965, 2277561536, 1044466142, 1764293411,
    2399213932, 393166890, 3808701840, 2903816909, 565674008, 3679292662, 3981333771, 556931443,
    4153856112, 1942146958, 3591288734, 2397456569, 273739313, 1015116575, 1282217977, 69049475,
    3082438817, 2738811899, 247623087, 3428478922, 3134464094, 463846664, 3724149500, 1722235603,
];
