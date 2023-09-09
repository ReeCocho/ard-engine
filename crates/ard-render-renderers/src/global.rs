use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_si::bindings::*;

/// Descriptor set bindings for frame global data. Used by all kinds of renderers.
pub struct GlobalSets {
    /// The actual per FIF sets.
    sets: Vec<DescriptorSet>,
}

impl GlobalSets {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        let sets = (0..frames_in_flight)
            .into_iter()
            .map(|_| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.global.clone(),
                        debug_name: Some("global_set_{frame_idx}".into()),
                    },
                )
                .unwrap()
            })
            .collect();

        Self { sets }
    }

    pub fn update_object_bindings(
        &mut self,
        frame: Frame,
        object_data: &Buffer,
        object_ids: &Buffer,
    ) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: GLOBAL_SET_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_OBJECT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids,
                    array_element: 0,
                },
            },
        ]);
    }
}
