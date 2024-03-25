use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_si::bindings::*;

use crate::ids::RenderIds;

pub struct HzbPassSets {
    sets: Vec<DescriptorSet>,
}

impl HzbPassSets {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        let sets = (0..frames_in_flight)
            .map(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.hzb_pass.clone(),
                        debug_name: Some(format!("hzb_pass_set_{frame_idx}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self { sets }
    }

    pub fn update_object_data_bindings(
        &mut self,
        frame: Frame,
        object_data: &Buffer,
        object_ids: &RenderIds,
    ) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: HZB_PASS_SET_GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: HZB_PASS_SET_INPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids.input(),
                    array_element: usize::from(frame),
                },
            },
            DescriptorSetUpdate {
                binding: HZB_PASS_SET_OUTPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids.output(),
                    array_element: 0,
                },
            },
        ]);
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }
}
