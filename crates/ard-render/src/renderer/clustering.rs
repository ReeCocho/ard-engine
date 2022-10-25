use ard_math::Vec4;
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};

use crate::shader_constants::{FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS, MAX_LIGHTS_PER_FROXEL};

use super::render_data::GlobalRenderData;

pub(super) const LIGHT_CLUSTERING_LIGHTS_BINDING: u32 = 0;
pub(super) const LIGHT_CLUSTERING_CLUSTERS_BINDING: u32 = 1;
pub(super) const LIGHT_CLUSTERING_CAMERA_BINDING: u32 = 2;
pub(super) const LIGHT_CLUSTERING_FROXELS_BINDING: u32 = 3;

pub(super) const FROXEL_GEN_CAMERA_BINDING: u32 = 0;
pub(super) const FROXEL_GEN_CLUSTERS_BINDING: u32 = 1;

pub(crate) struct LightClustering {
    /// Light clustering descriptor sets.
    pub light_cluster_sets: Vec<DescriptorSet>,
    /// Camera froxel generation descriptor sets.
    pub froxel_gen_sets: Vec<DescriptorSet>,
    /// Storage buffer for camera froxels.
    pub camera_froxels: Buffer,
    /// Storage buffer for lights.
    pub light_table: Buffer,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct Froxel {
    pub planes: [Vec4; 4],
    pub min_max_z: Vec4,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct CameraFroxels {
    pub frustums: [Froxel; FROXEL_TABLE_DIMS.0 * FROXEL_TABLE_DIMS.1 * FROXEL_TABLE_DIMS.2],
}

/// Table that contains the number of lights for each cluster and the light indicies.
#[repr(C)]
#[derive(Copy, Clone)]
struct LightTable {
    pub light_counts: [u32; FROXEL_TABLE_DIMS.0 * FROXEL_TABLE_DIMS.1 * FROXEL_TABLE_DIMS.2],
    pub clusters: [u32; FROXEL_TABLE_DIMS.0
        * FROXEL_TABLE_DIMS.1
        * FROXEL_TABLE_DIMS.2
        * MAX_LIGHTS_PER_FROXEL],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(super) struct LightClusteringPushConstants {
    light_count: u32,
}

unsafe impl Zeroable for LightTable {}
unsafe impl Pod for LightTable {}

unsafe impl Zeroable for LightClusteringPushConstants {}
unsafe impl Pod for LightClusteringPushConstants {}

unsafe impl Pod for CameraFroxels {}
unsafe impl Zeroable for CameraFroxels {}

unsafe impl Pod for Froxel {}
unsafe impl Zeroable for Froxel {}

impl LightClustering {
    pub fn new(
        ctx: &Context,
        name: &str,
        camera_ubo: &Buffer,
        light_cluster_layout: &DescriptorSetLayout,
        froxel_gen_layout: &DescriptorSetLayout,
    ) -> Self {
        let light_table = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<LightTable>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("light_table")),
            },
        )
        .unwrap();

        let camera_froxels = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<CameraFroxels>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("camera_froxels")),
            },
        )
        .unwrap();

        // Create light clustering sets
        let mut light_cluster_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: light_cluster_layout.clone(),
                    debug_name: Some(format!("{name}_light_cluster_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: LIGHT_CLUSTERING_CAMERA_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &camera_ubo,
                        array_element: frame,
                    },
                },
                DescriptorSetUpdate {
                    binding: LIGHT_CLUSTERING_CLUSTERS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &light_table,
                        array_element: frame,
                    },
                },
                DescriptorSetUpdate {
                    binding: LIGHT_CLUSTERING_FROXELS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &camera_froxels,
                        array_element: frame,
                    },
                },
            ]);

            light_cluster_sets.push(set);
        }

        // Create froxel generation sets
        let mut froxel_gen_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: froxel_gen_layout.clone(),
                    debug_name: Some(format!("{name}_froxel_gen_set{frame}")),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: FROXEL_GEN_CAMERA_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &camera_ubo,
                        array_element: frame,
                    },
                },
                DescriptorSetUpdate {
                    binding: FROXEL_GEN_CLUSTERS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &camera_froxels,
                        array_element: frame,
                    },
                },
            ]);

            froxel_gen_sets.push(set);
        }

        Self {
            light_cluster_sets,
            froxel_gen_sets,
            camera_froxels,
            light_table,
        }
    }

    pub fn update_light_clustering_set(&mut self, frame: usize, global: &GlobalRenderData) {
        let set = &mut self.light_cluster_sets[frame];
        set.update(&[DescriptorSetUpdate {
            binding: LIGHT_CLUSTERING_LIGHTS_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: &global.lights,
                array_element: frame,
            },
        }])
    }

    /// Dispatches a compute pass to generate camera froxels.
    pub fn generate_camera_froxels<'a>(
        &'a self,
        frame: usize,
        global: &GlobalRenderData,
        commands: &mut CommandBuffer<'a>,
    ) {
        commands.compute_pass(|pass| {
            pass.bind_pipeline(global.froxel_gen_pipeline.clone());
            pass.bind_sets(0, vec![&self.froxel_gen_sets[frame]]);
            pass.dispatch(
                FROXEL_TABLE_DIMS.0 as u32,
                FROXEL_TABLE_DIMS.1 as u32,
                FROXEL_TABLE_DIMS.2 as u32,
            );
        });
    }

    /// Dispatches a compute pass to cluster lights.
    pub fn cluster_lights<'a>(
        &'a self,
        frame: usize,
        global: &GlobalRenderData,
        commands: &mut CommandBuffer<'a>,
    ) {
        commands.compute_pass(|pass| {
            pass.bind_pipeline(global.light_cluster_pipeline.clone());
            pass.bind_sets(0, vec![&self.light_cluster_sets[frame]]);

            let constants = [LightClusteringPushConstants {
                light_count: global.light_count as u32,
            }];
            pass.push_constants(bytemuck::cast_slice(&constants));
            pass.dispatch(1, 1, FROXEL_TABLE_DIMS.2 as u32);
        });
    }
}
