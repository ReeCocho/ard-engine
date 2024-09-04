use std::ops::DerefMut;

use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec3Swizzles, Vec4};
use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, Context, DescriptorSet, DescriptorSetCreateInfo,
    DescriptorSetUpdate, DescriptorValue, MemoryUsage, QueueTypes, SharingMode,
};
use ard_render_base::{Frame, FRAMES_IN_FLIGHT};
use ard_render_si::{
    bindings::*,
    types::{GpuCamera, GpuFroxels},
};
use ard_transform::Model;

use crate::Camera;

#[derive(Resource, Component)]
pub struct CameraUbo {
    last_camera: Camera,
    last_model: Model,
    last_dims: (u32, u32),
    /// The actual UBO.
    ubo: Buffer,
    /// Froxels for light binning.
    froxels: Buffer,
    /// Descriptor set for the camera UBO for each frame in flight.
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    /// Descriptor sets for froxel regeneration.
    froxel_regen_sets: Vec<DescriptorSet>,
    /// Flag indicating if camera froxels need to be regenerated.
    froxel_regen: bool,
}

impl CameraUbo {
    pub fn new(ctx: &Context, has_froxels: bool, layouts: &Layouts) -> Self {
        let ubo = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: 6 * std::mem::size_of::<GpuCamera>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Concurrent,
                debug_name: Some("camera_ubo".to_owned()),
            },
        )
        .unwrap();

        let froxels = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: if has_froxels {
                    std::mem::size_of::<GpuFroxels>() as u64
                } else {
                    1
                },
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Concurrent,
                debug_name: Some("camera_froxels".to_owned()),
            },
        )
        .unwrap();

        let sets = std::array::from_fn(|frame_idx| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.camera.clone(),
                    debug_name: Some(format!("camera_set_{frame_idx}")),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: CAMERA_SET_CAMERA_UBO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &ubo,
                        array_element: frame_idx,
                    },
                },
                DescriptorSetUpdate {
                    binding: CAMERA_SET_CAMERA_FROXELS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &froxels,
                        array_element: 0,
                    },
                },
            ]);

            set
        });

        let froxel_regen_sets = if has_froxels {
            (0..FRAMES_IN_FLIGHT)
                .map(|frame_idx| {
                    let mut set = DescriptorSet::new(
                        ctx.clone(),
                        DescriptorSetCreateInfo {
                            layout: layouts.froxel_gen.clone(),
                            debug_name: Some(format!("froxel_regen_set_{frame_idx}")),
                        },
                    )
                    .unwrap();

                    set.update(&[
                        DescriptorSetUpdate {
                            binding: FROXEL_GEN_SET_CAMERA_UBO_BINDING,
                            array_element: 0,
                            value: DescriptorValue::UniformBuffer {
                                buffer: &ubo,
                                array_element: frame_idx,
                            },
                        },
                        DescriptorSetUpdate {
                            binding: FROXEL_GEN_SET_CAMERA_FROXELS_BINDING,
                            array_element: 0,
                            value: DescriptorValue::StorageBuffer {
                                buffer: &froxels,
                                array_element: 0,
                            },
                        },
                    ]);

                    set
                })
                .collect()
        } else {
            Vec::default()
        };

        Self {
            last_camera: Camera {
                near: f32::NEG_INFINITY,
                ..Default::default()
            },
            last_model: Model(Mat4::IDENTITY),
            last_dims: (0, 0),
            ubo,
            froxels,
            sets,
            froxel_regen: true,
            froxel_regen_sets,
        }
    }

    #[inline(always)]
    pub fn last(&self) -> &Camera {
        &self.last_camera
    }

    #[inline(always)]
    pub fn ubo(&self) -> &Buffer {
        &self.ubo
    }

    #[inline(always)]
    pub fn froxels(&self) -> &Buffer {
        &self.froxels
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }

    #[inline(always)]
    pub fn froxel_regen_set(&self, frame: Frame) -> &DescriptorSet {
        &self.froxel_regen_sets[usize::from(frame)]
    }

    #[inline(always)]
    pub fn needs_froxel_regen(&self) -> bool {
        !self.froxel_regen_sets.is_empty() && self.froxel_regen
    }

    pub fn update(&mut self, frame: Frame, value: &Camera, width: u32, height: u32, model: Model) {
        self.froxel_regen = false;

        if self.last_camera.needs_froxel_regen(value) || self.last_dims != (width, height) {
            self.froxel_regen = true;
        }

        let last_vp = self
            .last_camera
            .into_gpu_struct(width as f32, height as f32, self.last_model)
            .vp;
        let last_position = Vec4::from((self.last_model.position().xyz(), 1.0));

        self.last_dims = (width, height);
        self.last_model = model;
        self.last_camera = value.clone();

        /*
        let x = value.into_gpu_struct(width as f32, height as f32, model);

        let p = ard_math::Vec4::new(0.0, 0.0, 0.01, 1.0);
        let mut o = x.vp * p;
        o /= o.w;
        println!("================");
        println!("{}", o.z);

        let p = ard_math::Vec4::new(0.0, 0.0, 0.0, 1.0);
        let mut o = x.vp * p;
        o /= o.w;
        println!("{}", o.z);

        let p = ard_math::Vec4::new(0.0, 0.0, -0.01, 1.0);
        let mut o = x.vp * p;
        o /= o.w;
        println!("{}", o.z);
        */

        let mut new_gpu_cam = value.into_gpu_struct(width as f32, height as f32, model);
        new_gpu_cam.last_vp = last_vp;
        new_gpu_cam.last_position = last_position;

        let mut view = self.ubo.write(frame.into()).unwrap();
        bytemuck::cast_slice_mut::<_, GpuCamera>(view.deref_mut())[0] = new_gpu_cam;
    }

    pub fn update_raw(&mut self, frame: Frame, value: &GpuCamera, idx: usize) {
        let mut view = self.ubo.write(frame.into()).unwrap();
        bytemuck::cast_slice_mut::<_, GpuCamera>(view.deref_mut())[idx] = *value;
    }
}
