use ard_math::Vec2;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_objects::objects::RenderObjects;
use ard_render_si::{bindings::*, types::*};

/// Number of objects processed per workgroup.
const DRAW_GEN_WORKGROUP_SIZE: u32 = 64;

/// Pipeline for generating draw calls.
pub struct DrawGenPipeline {
    ctx: Context,
    pipeline: ComputePipeline,
    layout: DescriptorSetLayout,
}

/// Sets per frame in flight to be used when generating draw calls.
pub struct DrawGenSets {
    object_count: usize,
    sets: Vec<DescriptorSet>,
}

impl DrawGenPipeline {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./draw_gen.comp.spv")),
                debug_name: Some("draw_gen_shader".into()),
            },
        )
        .unwrap();

        let pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.draw_gen.clone(), layouts.camera.clone()],
                module,
                work_group_size: (DRAW_GEN_WORKGROUP_SIZE, 1, 1),
                push_constants_size: Some(std::mem::size_of::<GpuDrawGenPushConstants>() as u32),
                debug_name: Some("draw_gen_pipeline".into()),
            },
        )
        .unwrap();

        Self {
            ctx: ctx.clone(),
            pipeline,
            layout: layouts.draw_gen.clone(),
        }
    }

    pub fn generate<'a>(
        &self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        sets: &'a DrawGenSets,
        camera: &'a CameraUbo,
        render_area: Vec2,
    ) {
        commands.compute_pass(|pass| {
            pass.bind_pipeline(self.pipeline.clone());
            pass.bind_sets(0, vec![sets.get(frame), camera.get_set(frame)]);

            // Determine the number of groups to dispatch
            let object_count = sets.object_count as u32;
            let group_count = if object_count as u32 % DRAW_GEN_WORKGROUP_SIZE != 0 {
                (object_count as u32 / DRAW_GEN_WORKGROUP_SIZE) + 1
            } else {
                object_count as u32 / DRAW_GEN_WORKGROUP_SIZE
            }
            .max(1);

            let constants = [GpuDrawGenPushConstants {
                object_count,
                render_area,
            }];

            pass.push_constants(bytemuck::cast_slice(&constants));
            pass.dispatch(group_count, 1, 1);
        });
    }
}

impl DrawGenSets {
    pub fn new(pipeline: &DrawGenPipeline, frames_in_flight: usize) -> Self {
        let sets = (0..frames_in_flight)
            .into_iter()
            .map(|frame_idx| {
                DescriptorSet::new(
                    pipeline.ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: pipeline.layout.clone(),
                        debug_name: Some(format!("draw_gen_set_{frame_idx}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self {
            sets,
            object_count: 0,
        }
    }

    #[inline(always)]
    pub fn get(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }

    pub fn update_bindings(
        &mut self,
        frame: Frame,
        object_count: usize,
        draw_calls: (&Buffer, usize),
        objects: &RenderObjects,
        input_ids: &Buffer,
        output_ids: &Buffer,
    ) {
        self.object_count = object_count;
        self.sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: DRAW_GEN_SET_DRAW_CALLS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: draw_calls.0,
                    array_element: draw_calls.1,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_SET_OBJECTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: objects.object_data(),
                    array_element: usize::from(frame),
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_SET_INPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: input_ids,
                    array_element: usize::from(frame),
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_SET_OUTPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: output_ids,
                    array_element: 0,
                },
            },
        ]);
    }
}
