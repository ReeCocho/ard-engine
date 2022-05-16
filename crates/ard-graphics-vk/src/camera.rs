use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ash::vk;
use bytemuck::{Pod, Zeroable};

pub use crate::prelude::*;

use self::{
    alloc::{StorageBuffer, UniformBuffer},
    container::EscapeHandle,
    descriptors::DescriptorPool,
    forward_plus::POINT_LIGHTS_TABLE_DIMS,
    renderer::graph::FRAMES_IN_FLIGHT,
};

#[derive(Clone)]
pub struct Camera {
    pub(crate) id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CameraInner {
    pub descriptor: CameraDescriptor,
    pub ubo: UniformBuffer,
    pub cluster_ssbo: StorageBuffer,
    pub aligned_cluster_size: u64,
    pub set: vk::DescriptorSet,
    /// If the index here is not the same as the resize index for the render graph, we know that
    /// the cluster ssbo needs regeneration.
    pub cluster_regen_idx: [u32; FRAMES_IN_FLIGHT],
}

/// Camera data sent to shaders.
#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct CameraUBO {
    pub view: Mat4,
    pub projection: Mat4,
    pub vp: Mat4,
    pub view_inv: Mat4,
    pub projection_inv: Mat4,
    pub vp_inv: Mat4,
    pub frustum: Frustum,
    /// `x` = fov. `y` = near clipping plane. `z` = far clipping plane.
    pub properties: Vec4,
    pub position: Vec4,
    /// Scale and bias for clustered lighting.
    pub cluster_scale_bias: Vec2,
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
    pub frustums:
        [Froxel; POINT_LIGHTS_TABLE_DIMS.0 * POINT_LIGHTS_TABLE_DIMS.1 * POINT_LIGHTS_TABLE_DIMS.2],
}

unsafe impl Pod for CameraUBO {}
unsafe impl Zeroable for CameraUBO {}

unsafe impl Pod for CameraLightClusters {}
unsafe impl Zeroable for CameraLightClusters {}

impl CameraInner {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        camera_pool: &mut DescriptorPool,
        create_info: &CameraCreateInfo,
    ) -> Self {
        // Create UBO
        let ubo = UniformBuffer::new(ctx, CameraUBO::default());

        // Create cluster SSBO
        let min_alignment = ctx.0.properties.limits.min_uniform_buffer_offset_alignment;
        let aligned_size = match min_alignment {
            0 => std::mem::size_of::<CameraLightClusters>() as u64,
            align => {
                let align_mask = align - 1;
                (std::mem::size_of::<CameraLightClusters>() as u64 + align_mask) & !align_mask
            }
        } as usize;

        let cluster_ssbo = StorageBuffer::new(ctx, aligned_size * FRAMES_IN_FLIGHT);

        let set = camera_pool.allocate();

        let buffer_infos = [
            vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(ubo.size())
                .buffer(ubo.buffer())
                .build(),
            vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(aligned_size as vk::DeviceSize)
                .buffer(cluster_ssbo.buffer())
                .build(),
        ];

        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .dst_set(set)
                .buffer_info(&buffer_infos[0..1])
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER_DYNAMIC)
                .dst_set(set)
                .buffer_info(&buffer_infos[1..2])
                .build(),
        ];

        ctx.0.device.update_descriptor_sets(&writes, &[]);

        CameraInner {
            descriptor: create_info.descriptor,
            aligned_cluster_size: aligned_size as u64,
            set,
            ubo,
            cluster_ssbo,
            cluster_regen_idx: [0; FRAMES_IN_FLIGHT],
        }
    }

    #[inline]
    pub unsafe fn update(&mut self, frame: usize, width: f32, height: f32) {
        self.ubo
            .write(CameraUBO::new(&self.descriptor, width, height), frame);
    }
}

impl CameraApi for Camera {}

impl CameraUBO {
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

        CameraUBO {
            view,
            projection,
            vp,
            view_inv: view.inverse(),
            projection_inv: projection.inverse(),
            vp_inv: vp.inverse(),
            frustum: vp.into(),
            properties: Vec4::new(descriptor.fov, descriptor.near, descriptor.far, 0.0),
            position: Vec4::new(
                descriptor.position.x,
                descriptor.position.y,
                descriptor.position.z,
                0.0,
            ),
            cluster_scale_bias: Vec2::new(
                (POINT_LIGHTS_TABLE_DIMS.2 as f32) / (descriptor.far / descriptor.near).ln(),
                ((POINT_LIGHTS_TABLE_DIMS.2 as f32) * descriptor.near.ln())
                    / (descriptor.far / descriptor.near).ln(),
            ),
        }
    }
}
