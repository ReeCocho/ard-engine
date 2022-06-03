use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ash::vk;
use bytemuck::{Pod, Zeroable};

pub use crate::prelude::*;

use self::{
    alloc::{StorageBuffer, UniformBuffer},
    container::EscapeHandle,
    descriptors::DescriptorPool,
    shader_constants::{FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS},
};

#[derive(Clone)]
pub struct Camera {
    pub(crate) id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CameraInner {
    pub descriptor: CameraDescriptor,
}

/// Camera data sent to shaders.
#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct CameraUbo {
    pub view: Mat4,
    pub projection: Mat4,
    pub vp: Mat4,
    pub view_inv: Mat4,
    pub projection_inv: Mat4,
    pub vp_inv: Mat4,
    pub frustum: Frustum,
    pub position: Vec4,
    /// Scale and bias for clustered lighting.
    pub cluster_scale_bias: Vec2,
    pub fov: f32,
    pub near_clip: f32,
    pub far_clip: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct Froxel {
    pub planes: [Vec4; 4],
    pub min_max_z: Vec4,
}

/// SSBO containing frustums for every light cluster in the camera.
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct CameraLightClusters {
    pub frustums: [Froxel; FROXEL_TABLE_DIMS.0 * FROXEL_TABLE_DIMS.1 * FROXEL_TABLE_DIMS.2],
}

unsafe impl Pod for CameraUbo {}
unsafe impl Zeroable for CameraUbo {}

unsafe impl Pod for CameraLightClusters {}
unsafe impl Zeroable for CameraLightClusters {}

impl CameraInner {
    pub unsafe fn new(ctx: &GraphicsContext, create_info: &CameraCreateInfo) -> Self {
        CameraInner {
            descriptor: create_info.descriptor,
        }
    }
}

impl CameraApi for Camera {}

impl CameraUbo {
    pub fn new(descriptor: &CameraDescriptor, width: f32, height: f32) -> Self {
        let view = Mat4::look_at_lh(
            descriptor.position,
            descriptor.center,
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
