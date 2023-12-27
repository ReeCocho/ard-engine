use ard_core::core::Disabled;
use ard_ecs::{entity::Entity, resource::Resource};
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_objects::Model;
use ard_render_si::{
    bindings::Layouts,
    types::{GpuLight, GpuLightTable},
};

use crate::{
    clustering::{LightClusteringPipeline, LightClusteringSet},
    Light,
};

const DEFAULT_LIGHT_COUNT: usize = 1;

#[derive(Resource)]
pub struct Lighting {
    /// Buffer containing the light cluster table.
    clusters: Buffer,
    /// Clustering pipeline.
    pipeline: LightClusteringPipeline,
    /// Clustering set.
    sets: Vec<LightClusteringSet>,
}

pub struct Lights {
    lights: Buffer,
    count: usize,
}

impl Lighting {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        let clusters = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<GpuLightTable>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("light_clusters".into()),
            },
        )
        .unwrap();

        let pipeline = LightClusteringPipeline::new(ctx, layouts);

        let sets = (0..frames_in_flight)
            .into_iter()
            .map(|_| LightClusteringSet::new(ctx, layouts))
            .collect();

        Self {
            clusters,
            pipeline,
            sets,
        }
    }

    #[inline(always)]
    pub fn clusters(&self) -> &Buffer {
        &self.clusters
    }

    #[inline(always)]
    pub fn update_set(&mut self, frame: Frame, lights: &Lights) {
        self.sets[usize::from(frame)].update(lights, &self.clusters);
    }

    #[inline]
    pub fn cluster<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        frame: Frame,
        camera: &'a CameraUbo,
    ) {
        self.pipeline.cluster(
            commands,
            &self.sets[usize::from(frame)],
            camera.get_set(frame),
        );
    }
}

impl Lights {
    pub fn new(ctx: &Context) -> Self {
        Self {
            lights: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (std::mem::size_of::<GpuLight>() * DEFAULT_LIGHT_COUNT) as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("lights".into()),
                },
            )
            .unwrap(),
            count: 0,
        }
    }

    #[inline(always)]
    pub fn buffer(&self) -> &Buffer {
        &self.lights
    }

    #[inline(always)]
    pub fn light_count(&self) -> usize {
        self.count
    }

    pub fn update<'a>(
        &mut self,
        lights: impl ExactSizeIterator<Item = (Entity, (&'a Light, &'a Model), Option<&'a Disabled>)>,
    ) {
        // Resize the light buffer if needed
        let req_cap = (std::mem::size_of::<GpuLight>() * lights.len()) as u64;
        if let Some(new_buffer) = Buffer::expand(&self.lights, req_cap, false) {
            self.lights = new_buffer;
        }

        let mut view = self.lights.write(0).unwrap();
        self.count = 0;

        for (_, (light, mdl), disabled) in lights.into_iter() {
            if disabled.is_some() {
                continue;
            }

            view.set_as_array(
                light.to_gpu_light(mdl.position().into(), mdl.forward()),
                self.count,
            );

            self.count += 1;
        }
    }
}
