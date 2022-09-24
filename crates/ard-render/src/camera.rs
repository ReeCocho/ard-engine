use ard_ecs::prelude::Component;
use ard_math::{Mat4, Vec3, Vec4};
use ard_pal::prelude::{ClearColor, Context};
use bytemuck::{Pod, Zeroable};

use crate::{
    factory::{
        allocator::{EscapeHandle, ResourceId},
        Layouts,
    },
    renderer::{render_data::RenderData, RenderLayer},
};

#[derive(Debug, Clone, Copy)]
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
    pub clear_color: Option<Vec3>,
    /// The layers this camera renders.
    pub layers: RenderLayer,
}

#[derive(Clone)]
pub struct Camera {
    pub(crate) id: ResourceId,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CameraInner {
    pub descriptor: CameraDescriptor,
    pub render_data: RenderData,
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
    pub position: Vec4,
    pub fov: f32,
    pub near_clip: f32,
    pub far_clip: f32,
}

unsafe impl Pod for CameraUbo {}
unsafe impl Zeroable for CameraUbo {}

impl CameraInner {
    pub fn new(ctx: &Context, descriptor: CameraDescriptor, layouts: &Layouts) -> Self {
        Self {
            descriptor,
            render_data: RenderData::new(ctx, "camera", &layouts.global, &layouts.draw_gen),
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
        let projection = Mat4::perspective_lh(
            descriptor.fov,
            width / height,
            descriptor.near,
            descriptor.far,
        );
        let vp = projection * view;

        CameraUbo {
            view,
            projection,
            vp,
            view_inv: view.inverse(),
            projection_inv: projection.inverse(),
            vp_inv: vp.inverse(),
            position: Vec4::new(
                descriptor.position.x,
                descriptor.position.y,
                descriptor.position.z,
                0.0,
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
            far: 100.0,
            fov: 80.0f32.to_radians(),
            order: 0,
            clear_color: Some(Vec3::ZERO),
            layers: RenderLayer::OPAQUE,
        }
    }
}
