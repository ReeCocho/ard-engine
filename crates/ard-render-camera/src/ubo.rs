use std::ops::DerefMut;

use ard_ecs::prelude::*;
use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, Context, DescriptorSet, DescriptorSetCreateInfo,
    DescriptorSetUpdate, DescriptorValue, MemoryUsage,
};
use ard_render_base::ecs::Frame;
use ard_render_objects::Model;
use ard_render_si::{
    bindings::*,
    types::{GpuCamera, GpuFroxels},
};

use crate::Camera;

#[derive(Resource, Component)]
pub struct CameraUbo {
    last_camera: Camera,
    last_dims: (u32, u32),
    /// The actual UBO.
    ubo: Buffer,
    /// Froxels for light binning.
    _froxels: Buffer,
    /// Descriptor set for the camera UBO for each frame in flight.
    sets: Vec<DescriptorSet>,
    /// Descriptor sets for froxel regeneration.
    froxel_regen_sets: Vec<DescriptorSet>,
    /// Flag indicating if camera froxels need to be regenerated.
    froxel_regen: bool,
}

impl CameraUbo {
    pub fn new(ctx: &Context, fif: usize, layouts: &Layouts) -> Self {
        let ubo = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<GpuCamera>() as u64,
                array_elements: fif,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some("camera_ubo".to_owned()),
            },
        )
        .unwrap();

        let froxels = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<GpuFroxels>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some("camera_froxels".to_owned()),
            },
        )
        .unwrap();

        let sets = (0..fif)
            .into_iter()
            .map(|frame_idx| {
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
            })
            .collect();

        let froxel_regen_sets = (0..fif)
            .into_iter()
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
            .collect();

        Self {
            last_camera: Camera {
                near: f32::NEG_INFINITY,
                ..Default::default()
            },
            last_dims: (0, 0),
            ubo,
            _froxels: froxels,
            sets,
            froxel_regen: true,
            froxel_regen_sets,
        }
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
        self.froxel_regen
    }

    pub fn update(&mut self, frame: Frame, value: &Camera, width: u32, height: u32, model: Model) {
        self.froxel_regen = false;

        if self.last_camera.needs_froxel_regen(value) || self.last_dims != (width, height) {
            self.froxel_regen = true;
        }
        self.last_dims = (width, height);
        self.last_camera = value.clone();

        let mut view = self.ubo.write(frame.into()).unwrap();
        bytemuck::cast_slice_mut::<_, GpuCamera>(view.deref_mut())[0] =
            value.into_gpu_struct(width as f32, height as f32, model);
    }
}