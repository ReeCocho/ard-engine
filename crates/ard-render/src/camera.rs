use std::sync::atomic::{AtomicBool, Ordering};

use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ard_pal::prelude::Context;
use bytemuck::{Pod, Zeroable};

use crate::{
    cube_map::CubeMap,
    factory::{
        allocator::{EscapeHandle, ResourceId},
        Layouts,
    },
    renderer::{ao::AoImage, render_data::RenderData, shadows::Shadows, RenderLayer},
    shader_constants::{FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS},
};

#[derive(Clone)]
pub struct CameraDescriptor {
    /// The global position of the camera.
    pub position: Vec3,
    /// The global position the camera is looking at.
    pub target: Vec3,
    /// Up vector for the orientation of the camera.
    pub up: Vec3,
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
    /// The layers this camera renders.
    pub layers: RenderLayer,
    /// If this camera should render shadow maps.
    pub shadows: Option<CameraShadows>,
    /// If this camera should use ambient occlusion.
    pub ao: bool,
}

#[derive(Clone)]
pub enum CameraClearColor {
    /// No clearin is done.
    None,
    /// The screen is cleared a solid color.
    Color(Vec3),
    /// A skybox is used as the background.
    SkyBox(CubeMap),
}

#[derive(Debug, Copy, Clone)]
pub struct CameraShadows {
    /// The resolution of the base shadow map.
    pub resolution: u32,
    /// The number of cascades to use.
    pub cascades: usize,
}

/// Describes a view frustum using planes.
#[derive(Debug, Default, Copy, Clone)]
#[repr(C)]
pub struct Frustum {
    /// Planes come in the following order:
    /// - Left
    /// - Right
    /// - Top
    /// - Bottom
    /// - Near
    /// - Far
    /// With inward facing normals.
    pub planes: [Vec4; 6],
}

#[derive(Clone)]
pub struct Camera {
    pub(crate) id: ResourceId,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CameraInner {
    pub descriptor: CameraDescriptor,
    pub render_data: RenderData,
    pub shadows: Shadows,
    pub ao: AoImage,
    /// Flags to indicate if a froxel regen is required.
    pub regen_froxels: [AtomicBool; FRAMES_IN_FLIGHT],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CameraUbo {
    pub view: Mat4,
    pub projection: Mat4,
    pub vp: Mat4,
    pub view_inv: Mat4,
    pub projection_inv: Mat4,
    pub vp_inv: Mat4,
    pub frustum: Frustum,
    pub position: Vec4,
    pub cluster_scale_bias: Vec2,
    pub fov: f32,
    pub near_clip: f32,
    pub far_clip: f32,
}

unsafe impl Pod for CameraUbo {}
unsafe impl Zeroable for CameraUbo {}

unsafe impl Pod for Frustum {}
unsafe impl Zeroable for Frustum {}

impl CameraInner {
    pub fn new(ctx: &Context, descriptor: CameraDescriptor, layouts: &Layouts) -> Self {
        let shadows = match &descriptor.shadows {
            Some(shadows) => Shadows::new(ctx, layouts, shadows.resolution, shadows.cascades),
            None => Shadows::new(ctx, layouts, 0, 0),
        };

        Self {
            descriptor,
            shadows,
            ao: AoImage::new(ctx, &layouts.ao),
            render_data: RenderData::new(ctx, "camera", &layouts, true),
            regen_froxels: Default::default(),
        }
    }

    #[inline(always)]
    pub fn needs_froxel_regen(&self, frame: usize) -> bool {
        self.regen_froxels[frame].swap(false, Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn mark_froxel_regen(&self) {
        for mark in &self.regen_froxels {
            mark.store(true, Ordering::Relaxed);
        }
    }
}

impl CameraUbo {
    pub fn new(descriptor: &CameraDescriptor, width: f32, height: f32) -> Self {
        debug_assert_ne!(width, 0.0);
        debug_assert_ne!(height, 0.0);

        let view = Mat4::look_at_lh(
            descriptor.position,
            descriptor.target,
            descriptor.up.try_normalize().unwrap_or(Vec3::Y),
        );
        let projection =
            Mat4::perspective_infinite_reverse_lh(descriptor.fov, width / height, descriptor.near);
        let vp = projection * view;

        CameraUbo {
            view,
            projection,
            vp,
            view_inv: view.inverse(),
            projection_inv: projection.inverse(),
            vp_inv: vp.inverse(),
            frustum: vp.into(),
            position: Vec4::new(
                descriptor.position.x,
                descriptor.position.y,
                descriptor.position.z,
                0.0,
            ),
            cluster_scale_bias: Vec2::new(
                (FROXEL_TABLE_DIMS.2 as f32) / (descriptor.far / descriptor.near).ln(),
                ((FROXEL_TABLE_DIMS.2 as f32) * descriptor.near.ln())
                    / (descriptor.far / descriptor.near).ln(),
            ),
            fov: descriptor.fov,
            near_clip: descriptor.near,
            far_clip: descriptor.far,
        }
    }
}

impl Default for CameraDescriptor {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            target: Vec3::Z,
            up: Vec3::Y,
            near: 0.03,
            far: 500.0,
            fov: 80.0f32.to_radians(),
            order: 0,
            clear_color: CameraClearColor::Color(Vec3::ZERO),
            layers: RenderLayer::OPAQUE,
            shadows: None,
            ao: false,
        }
    }
}

impl From<Mat4> for Frustum {
    fn from(m: Mat4) -> Frustum {
        let mut frustum = Frustum {
            planes: [
                m.row(3) + m.row(0),
                m.row(3) - m.row(0),
                m.row(3) - m.row(1),
                m.row(3) + m.row(1),
                m.row(2),
                m.row(3) - m.row(2),
            ],
        };

        for plane in &mut frustum.planes {
            *plane /= Vec4::new(plane.x, plane.y, plane.z, 0.0).length();
        }

        frustum
    }
}
