use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, FRAMES_IN_FLIGHT};
use ard_render_si::bindings::*;

use crate::{highz::HzbImage, ids::RenderIds};

pub struct DepthPrepassSets {
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

impl DepthPrepassSets {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        Self {
            sets: std::array::from_fn(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.depth_prepass.clone(),
                        debug_name: Some(format!("depth_prepass_set_{frame_idx}")),
                    },
                )
                .unwrap()
            }),
        }
    }

    pub fn update_hzb_binding(&mut self, frame: Frame, image: &HzbImage) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: DEPTH_PREPASS_SET_HZB_IMAGE_BINDING,
            array_element: 0,
            value: image.descriptor_value(),
        }]);
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
                binding: DEPTH_PREPASS_SET_GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: DEPTH_PREPASS_SET_INPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids.input(),
                    array_element: usize::from(frame),
                },
            },
            DescriptorSetUpdate {
                binding: DEPTH_PREPASS_SET_OUTPUT_IDS_BINDING,
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
