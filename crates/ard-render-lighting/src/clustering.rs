use ard_pal::prelude::*;
use ard_render_si::{
    bindings::{
        Layouts, LIGHT_CLUSTERING_SET_LIGHTS_BINDING, LIGHT_CLUSTERING_SET_LIGHT_CLUSTERS_BINDING,
    },
    consts::{CAMERA_FROXELS_DEPTH, CAMERA_FROXELS_HEIGHT, CAMERA_FROXELS_WIDTH},
    types::GpuLightClusteringPushConstants,
};

use crate::lights::Lights;

pub struct LightClusteringPipeline {
    pipeline: ComputePipeline,
}

pub struct LightClusteringSet {
    set: DescriptorSet,
    light_count: usize,
}

impl LightClusteringPipeline {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./light_clustering.comp.spv")),
                debug_name: Some("light_clustering_shader".into()),
            },
        )
        .unwrap();

        let pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.camera.clone(), layouts.light_clustering.clone()],
                module,
                work_group_size: (CAMERA_FROXELS_WIDTH as u32, CAMERA_FROXELS_HEIGHT as u32, 1),
                push_constants_size: Some(
                    std::mem::size_of::<GpuLightClusteringPushConstants>() as u32
                ),
                debug_name: Some("light_clustering_pipeline".into()),
            },
        )
        .unwrap();

        Self { pipeline }
    }

    pub fn cluster<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        set: &'a LightClusteringSet,
        camera_set: &'a DescriptorSet,
    ) {
        commands.compute_pass(&self.pipeline, Some("light_clustering"), |pass| {
            pass.bind_sets(0, vec![camera_set, set.get()]);

            let constants = [GpuLightClusteringPushConstants {
                total_lights: set.light_count() as u32,
            }];
            pass.push_constants(bytemuck::cast_slice(&constants));
            (1, 1, CAMERA_FROXELS_DEPTH as u32)
        });
    }
}

impl LightClusteringSet {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        Self {
            set: DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.light_clustering.clone(),
                    debug_name: Some(format!("light_clustering_set")),
                },
            )
            .unwrap(),
            light_count: 0,
        }
    }

    #[inline(always)]
    pub fn get(&self) -> &DescriptorSet {
        &self.set
    }

    #[inline(always)]
    pub fn light_count(&self) -> usize {
        self.light_count
    }

    pub fn bind_clusters(&mut self, clusters: &Buffer) {
        self.set.update(&[DescriptorSetUpdate {
            binding: LIGHT_CLUSTERING_SET_LIGHT_CLUSTERS_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: clusters,
                array_element: 0,
            },
        }]);
    }

    pub fn bind_lights(&mut self, lights: &Lights) {
        self.light_count = lights.light_count();

        self.set.update(&[DescriptorSetUpdate {
            binding: LIGHT_CLUSTERING_SET_LIGHTS_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: lights.buffer(),
                array_element: 0,
            },
        }]);
    }
}
