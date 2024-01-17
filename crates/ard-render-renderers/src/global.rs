use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_image_effects::ao::AO_SAMPLER;
use ard_render_si::{bindings::*, consts::*};

use crate::shadow::SHADOW_SAMPLER;

/// Descriptor set bindings for frame global data. Used by all kinds of renderers.
pub struct GlobalSets {
    /// The actual per FIF sets.
    sets: Vec<DescriptorSet>,
}

pub struct GlobalSetBindingUpdate<'a> {
    pub frame: Frame,
    pub object_data: &'a Buffer,
    pub object_ids: &'a Buffer,
    pub global_lighting: &'a Buffer,
    pub lights: &'a Buffer,
    pub clusters: &'a Buffer,
    pub sun_shadow_info: &'a Buffer,
    pub ao_image: &'a Texture,
    pub shadow_cascades: [&'a Texture; MAX_SHADOW_CASCADES],
}

impl GlobalSets {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        let sets = (0..frames_in_flight)
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

    pub fn update_object_bindings(&mut self, update: GlobalSetBindingUpdate) {
        let set = &mut self.sets[usize::from(update.frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: GLOBAL_SET_GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: update.object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_OBJECT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: update.object_ids,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_LIGHTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: update.lights,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_LIGHT_CLUSTERS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: update.clusters,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: update.global_lighting,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_SUN_SHADOW_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: update.sun_shadow_info,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_AO_IMAGE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: update.ao_image,
                    array_element: 0,
                    sampler: AO_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);

        let shadow_cascades_update: [_; MAX_SHADOW_CASCADES] =
            std::array::from_fn(|i| DescriptorSetUpdate {
                binding: GLOBAL_SET_SHADOW_CASCADES_BINDING,
                array_element: i,
                value: DescriptorValue::Texture {
                    texture: update.shadow_cascades[i],
                    array_element: 0,
                    sampler: SHADOW_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            });

        set.update(&shadow_cascades_update);
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }
}
