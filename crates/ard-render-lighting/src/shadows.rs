use std::ops::DerefMut;

use ard_math::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use ard_pal::prelude::*;
use ard_render_camera::Camera;
use ard_render_objects::Model;
use ard_render_si::{consts::*, types::*};

pub struct SunShadowsUbo {
    ubo: Buffer,
    cameras: [GpuCamera; MAX_SHADOW_CASCADES],
    cascades: usize,
}

impl SunShadowsUbo {
    pub fn new(ctx: &Context) -> Self {
        Self {
            ubo: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: std::mem::size_of::<GpuSunShadows>() as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::UNIFORM_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
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
                near_clip: 1.0,
                far_clip: 1.0,
                cluster_scale_bias: Vec2::ONE,
            }),
            cascades: 0,
        }
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
        cascades: usize,
        light_dir: Vec3,
        camera: &Camera,
        camera_model: Model,
        camera_aspect: f32,
        shadow_resolution: u32,
    ) {
        debug_assert!(cascades <= MAX_SHADOW_CASCADES);

        let mut buff_view = self.ubo.write(0).unwrap();
        let ubo = &mut bytemuck::cast_slice_mut::<_, GpuSunShadows>(buff_view.deref_mut())[0];

        self.cascades = cascades;
        ubo.count = cascades as u32;

        for i in 0..cascades {
            let lin_near = (i as f32 / cascades as f32).powf(2.0);
            let lin_far = ((i + 1) as f32 / cascades as f32).powf(2.0);

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
            let fmn = camera.far - camera.near;
            let far_plane = camera.near + (fmn * lin_far);
            let camera_proj = Mat4::perspective_lh(
                camera.fov,
                camera_aspect,
                camera.near + (fmn * lin_near),
                far_plane,
            );
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
            let texels_per_unit = shadow_resolution as f32 / (radius * 2.0);
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
                near_clip: 1.0,
                far_clip: 1.0,
                cluster_scale_bias: Vec2::ONE,
            };

            ubo.cascades[i] = GpuShadowCascade {
                vp,
                view,
                proj,
                // TODO: Make sun size configurable
                uv_size: 1.5 / (Vec2::ONE * 2.0 * radius),
                far_plane,
                // TODO: Max bias configurable
                min_bias: 0.05,
                max_bias: 0.2,
                depth_range: 2.0 * radius,
            };
        }
    }
}
