use ard_math::Vec2;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_objects::objects::RenderObjects;
use ard_render_si::{bindings::*, types::*};

use crate::highz::HzbImage;

/// Number of objects processed per workgroup.
const DRAW_GEN_WORKGROUP_SIZE: u32 = 64 * 2;
const DRAW_COMPACT_WORKGROUP_SIZE: u32 = 64 * 2;

/// Pipeline for generating draw calls.
pub struct DrawGenPipeline {
    ctx: Context,
    gen_pipeline: ComputePipeline,
    gen_no_hzb_pipeline: ComputePipeline,
    compact_pipeline: ComputePipeline,
    gen_layout: DescriptorSetLayout,
    gen_no_hzb_layout: DescriptorSetLayout,
    compact_layout: DescriptorSetLayout,
}

/// Sets per frame in flight to be used when generating draw calls.
pub struct DrawGenSets {
    use_hzb: bool,
    non_transparent_object_count: usize,
    object_count: usize,
    draw_count: usize,
    non_transparent_draw_count: usize,
    gen_sets: Vec<DescriptorSet>,
    compact_sets: Vec<DescriptorSet>,
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

        let gen_pipeline = ComputePipeline::new(
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

        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./draw_gen_no_hzb.comp.spv")),
                debug_name: Some("draw_gen_no_hzb_shader".into()),
            },
        )
        .unwrap();

        let gen_no_hzb_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.draw_gen_no_hzb.clone(), layouts.camera.clone()],
                module,
                work_group_size: (DRAW_GEN_WORKGROUP_SIZE, 1, 1),
                push_constants_size: Some(std::mem::size_of::<GpuDrawGenPushConstants>() as u32),
                debug_name: Some("draw_gen_no_hzb_pipeline".into()),
            },
        )
        .unwrap();

        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./draw_compact.comp.spv")),
                debug_name: Some("draw_compact_shader".into()),
            },
        )
        .unwrap();

        let compact_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.draw_compact.clone()],
                    module,
                    work_group_size: (DRAW_COMPACT_WORKGROUP_SIZE, 1, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuDrawCompactPushConstants>() as u32
                    ),
                    debug_name: Some("draw_compact_pipeline".into()),
                },
            )
            .unwrap();

        Self {
            ctx: ctx.clone(),
            gen_pipeline,
            gen_no_hzb_pipeline,
            compact_pipeline,
            gen_layout: layouts.draw_gen.clone(),
            gen_no_hzb_layout: layouts.draw_gen_no_hzb.clone(),
            compact_layout: layouts.draw_compact.clone(),
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
        // Generate draw calls
        commands.compute_pass(
            if sets.use_hzb {
                &self.gen_pipeline
            } else {
                &self.gen_no_hzb_pipeline
            },
            Some("generate_draw_calls"),
            |pass| {
                pass.bind_sets(0, vec![sets.get_gen(frame), camera.get_set(frame)]);

                let object_count = sets.object_count as u32;
                let group_count = object_count.div_ceil(DRAW_COMPACT_WORKGROUP_SIZE).max(1);

                let constants = [GpuDrawGenPushConstants {
                    object_count,
                    render_area,
                    transparent_start: sets.non_transparent_object_count as u32,
                }];

                pass.push_constants(bytemuck::cast_slice(&constants));
                (group_count, 1, 1)
            },
        );
    }

    pub fn compact<'a>(
        &self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        sets: &'a DrawGenSets,
    ) {
        // Compact non-transparent draw calls
        commands.compute_pass(&self.compact_pipeline, Some("compact_draw_calls"), |pass| {
            pass.bind_sets(0, vec![sets.get_compact(frame)]);

            let draw_count = sets.draw_count as u32;
            let group_count = draw_count.div_ceil(DRAW_COMPACT_WORKGROUP_SIZE).max(1);

            let constants = [GpuDrawCompactPushConstants {
                base_draw_call: 0,
                draw_call_count: draw_count,
                transparent_start: sets.non_transparent_draw_count as u32,
            }];

            pass.push_constants(bytemuck::cast_slice(&constants));
            (group_count, 1, 1)
        });
    }
}

impl DrawGenSets {
    pub fn new(pipeline: &DrawGenPipeline, use_hzb: bool, frames_in_flight: usize) -> Self {
        let gen_sets = (0..frames_in_flight)
            .map(|frame_idx| {
                DescriptorSet::new(
                    pipeline.ctx.clone(),
                    if use_hzb {
                        DescriptorSetCreateInfo {
                            layout: pipeline.gen_layout.clone(),
                            debug_name: Some(format!("draw_gen_set_{frame_idx}")),
                        }
                    } else {
                        DescriptorSetCreateInfo {
                            layout: pipeline.gen_no_hzb_layout.clone(),
                            debug_name: Some(format!("draw_gen_no_hzb_set_{frame_idx}")),
                        }
                    },
                )
                .unwrap()
            })
            .collect();

        let compact_sets = (0..frames_in_flight)
            .map(|frame_idx| {
                DescriptorSet::new(
                    pipeline.ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: pipeline.compact_layout.clone(),
                        debug_name: Some(format!("draw_compact_set_{frame_idx}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self {
            use_hzb,
            gen_sets,
            compact_sets,
            object_count: 0,
            non_transparent_object_count: 0,
            draw_count: 0,
            non_transparent_draw_count: 0,
        }
    }

    #[inline(always)]
    pub fn get_gen(&self, frame: Frame) -> &DescriptorSet {
        &self.gen_sets[usize::from(frame)]
    }

    #[inline(always)]
    pub fn get_compact(&self, frame: Frame) -> &DescriptorSet {
        &self.compact_sets[usize::from(frame)]
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_bindings<const FIF: usize>(
        &mut self,
        frame: Frame,
        object_count: usize,
        non_transparent_object_count: usize,
        draw_count: usize,
        non_transparent_draw_count: usize,
        src_draw_groups: (&Buffer, usize),
        instance_counts: (&Buffer, usize),
        dst_draw_calls: (&Buffer, usize),
        draw_counts: (&Buffer, usize),
        objects: &RenderObjects,
        hzb: Option<&HzbImage<FIF>>,
        input_ids: &Buffer,
        output_ids: &Buffer,
        mesh_info: &Buffer,
    ) {
        self.object_count = object_count;
        self.non_transparent_object_count = non_transparent_object_count;
        self.draw_count = draw_count;
        self.non_transparent_draw_count = non_transparent_draw_count;

        if self.use_hzb {
            self.gen_sets[usize::from(frame)].update(&[
                DescriptorSetUpdate {
                    binding: DRAW_GEN_SET_DRAW_GROUPS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: src_draw_groups.0,
                        array_element: src_draw_groups.1,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_SET_INSTANCE_COUNTS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: instance_counts.0,
                        array_element: instance_counts.1,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_SET_OBJECTS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: objects.object_data(),
                        array_element: 0,
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
                DescriptorSetUpdate {
                    binding: DRAW_GEN_SET_MESH_INFO_LOOKUP_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: mesh_info,
                        array_element: usize::from(frame),
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_SET_HZB_IMAGE_BINDING,
                    array_element: 0,
                    value: hzb.unwrap().descriptor_value(),
                },
            ]);
        } else {
            self.gen_sets[usize::from(frame)].update(&[
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_DRAW_GROUPS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: src_draw_groups.0,
                        array_element: src_draw_groups.1,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_INSTANCE_COUNTS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: instance_counts.0,
                        array_element: instance_counts.1,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_OBJECTS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: objects.object_data(),
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_INPUT_IDS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: input_ids,
                        array_element: usize::from(frame),
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_OUTPUT_IDS_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: output_ids,
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: DRAW_GEN_NO_HZB_SET_MESH_INFO_LOOKUP_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: mesh_info,
                        array_element: usize::from(frame),
                    },
                },
            ]);
        }

        self.compact_sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: DRAW_COMPACT_SET_DRAW_GROUPS_SRC_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: src_draw_groups.0,
                    array_element: src_draw_groups.1,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_COMPACT_SET_INSTANCE_COUNTS_SRC_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: instance_counts.0,
                    array_element: instance_counts.1,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_COMPACT_SET_DRAW_CALLS_DST_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: dst_draw_calls.0,
                    array_element: dst_draw_calls.1,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_COMPACT_SET_DRAW_COUNTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: draw_counts.0,
                    array_element: draw_counts.1,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_COMPACT_SET_MESH_INFO_LOOKUP_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: mesh_info,
                    array_element: usize::from(frame),
                },
            },
        ]);
    }
}
