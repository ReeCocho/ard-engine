use ard_ecs::prelude::Component;
use ard_math::{Mat4, Vec2, Vec3, Vec3A, Vec4, Vec4Swizzles};
use ard_render_objects::{Model, RenderFlags};
use ard_render_si::{consts::CAMERA_FROXELS_DEPTH, types::GpuCamera};

pub mod active;
pub mod froxels;
pub mod target;
pub mod ubo;

#[derive(Debug, Component, Clone)]
pub struct Camera {
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
    /// The ordering value for this camera. Cameras render their images from lowest to highest.
    /// Cameras with an equal ordering have an unspecified rendering order.
    pub order: i32,
    /// Clear color options for this camera.
    pub clear_color: CameraClearColor,
    /// Required flags entites must have for this camera to render them.
    pub flags: RenderFlags,
}

#[derive(Debug, Clone)]
pub enum CameraClearColor {
    /// Do not clear.
    None,
    /// Clear the screen using a solid color.
    Color(Vec4),
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            near: 0.06,
            far: 300.0,
            fov: 80.0_f32.to_radians(),
            order: 0,
            clear_color: CameraClearColor::Color(Vec4::ZERO),
            flags: RenderFlags::empty(),
        }
    }
}

impl Camera {
    /// Given a new camera, determines if the projection has changed, required froxels to be
    /// regenerated.
    #[inline]
    pub fn needs_froxel_regen(&self, new_camera: &Camera) -> bool {
        self.near != new_camera.near || self.far != new_camera.far || self.fov != new_camera.fov
    }

    /// Makes a GPU compatible version of the camera given render target dimensions and a model
    /// matrix describing the orientation of the camera.
    pub fn into_gpu_struct(&self, width: f32, height: f32, model: Model) -> GpuCamera {
        debug_assert_ne!(width, 0.0);
        debug_assert_ne!(height, 0.0);

        let position = model.position();
        let up = model.0.col(1).xyz().try_normalize().unwrap_or(Vec3::Y);
        let forward = model.0.col(2).xyz().try_normalize().unwrap_or(Vec3::Z);

        let view = Mat4::look_at_lh(
            position.into(),
            (position + Vec3A::from(forward)).into(),
            up,
        );
        let projection = Mat4::perspective_infinite_reverse_lh(self.fov, width / height, self.near);
        let vp = projection * view;

        GpuCamera {
            view,
            projection,
            vp,
            view_inv: view.inverse(),
            projection_inv: projection.inverse(),
            vp_inv: vp.inverse(),
            frustum: vp.into(),
            position: Vec4::new(position.x, position.y, position.z, 1.0),
            near_clip: self.near,
            far_clip: self.far,
            cluster_scale_bias: Vec2::new(
                (CAMERA_FROXELS_DEPTH as f32) / (self.far / self.near).ln(),
                ((CAMERA_FROXELS_DEPTH as f32) * self.near.ln()) / (self.far / self.near).ln(),
            ),
        }
    }
}
