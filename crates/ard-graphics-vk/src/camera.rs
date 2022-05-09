use ash::vk;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

pub use crate::prelude::*;

use self::{alloc::UniformBuffer, container::EscapeHandle, descriptors::DescriptorPool};

#[derive(Clone)]
pub struct Camera {
    pub(crate) id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CameraInner {
    pub descriptor: CameraDescriptor,
    pub ubo: UniformBuffer,
    pub set: vk::DescriptorSet,
}

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
}

unsafe impl Pod for CameraUBO {}
unsafe impl Zeroable for CameraUBO {}

impl CameraInner {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        camera_pool: &mut DescriptorPool,
        create_info: &CameraCreateInfo,
    ) -> Self {
        let ubo = UniformBuffer::new(ctx, CameraUBO::default());
        let set = camera_pool.allocate();

        let buffer_info = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(ubo.size())
            .buffer(ubo.buffer())
            .build()];

        let write = [vk::WriteDescriptorSet::builder()
            .dst_array_element(0)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .dst_set(set)
            .buffer_info(&buffer_info)
            .build()];

        ctx.0.device.update_descriptor_sets(&write, &[]);

        CameraInner {
            descriptor: create_info.descriptor,
            set,
            ubo,
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
        }
    }
}
