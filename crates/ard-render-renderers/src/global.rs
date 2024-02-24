use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_image_effects::ao::AO_SAMPLER;
use ard_render_lighting::proc_skybox::DI_MAP_SAMPLER;
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
            .map(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.global.clone(),
                        debug_name: Some(format!("global_set_{frame_idx}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self { sets }
    }

    pub fn update_shadow_bindings(
        &mut self,
        frame: Frame,
        sun_shadow_info: &Buffer,
        cascades: [&Texture; MAX_SHADOW_CASCADES],
    ) {
        let set = &mut self.sets[usize::from(frame)];

        let shadow_cascades_update: [_; MAX_SHADOW_CASCADES + 1] = std::array::from_fn(|i| {
            if i < MAX_SHADOW_CASCADES {
                DescriptorSetUpdate {
                    binding: GLOBAL_SET_SHADOW_CASCADES_BINDING,
                    array_element: i,
                    value: DescriptorValue::Texture {
                        texture: cascades[i],
                        array_element: 0,
                        sampler: SHADOW_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                }
            } else {
                DescriptorSetUpdate {
                    binding: GLOBAL_SET_SUN_SHADOW_INFO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: sun_shadow_info,
                        array_element: 0,
                    },
                }
            }
        });

        set.update(&shadow_cascades_update);
    }

    pub fn update_di_map_binding(&mut self, frame: Frame, di_map: &CubeMap) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: GLOBAL_SET_DI_MAP_BINDING,
            array_element: 0,
            value: DescriptorValue::CubeMap {
                cube_map: di_map,
                array_element: 0,
                sampler: DI_MAP_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn update_ao_image_binding(&mut self, frame: Frame, ao_image: &Texture) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: GLOBAL_SET_AO_IMAGE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: ao_image,
                array_element: 0,
                sampler: AO_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn update_light_clusters_binding(&mut self, frame: Frame, clusters: &Buffer) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: GLOBAL_SET_LIGHT_CLUSTERS_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: clusters,
                array_element: 0,
            },
        }]);
    }

    pub fn update_lighting_binding(
        &mut self,
        frame: Frame,
        global_lighting: &Buffer,
        lights: &Buffer,
    ) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: GLOBAL_SET_LIGHTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: lights,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: global_lighting,
                    array_element: 0,
                },
            },
        ]);
    }

    pub fn update_object_data_bindings(
        &mut self,
        frame: Frame,
        object_data: &Buffer,
        object_ids: &Buffer,
    ) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: GLOBAL_SET_GLOBAL_OBJECT_DATA_BINDING,
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

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }
}
