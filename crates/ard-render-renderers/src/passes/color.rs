use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_image_effects::ao::AO_SAMPLER;
use ard_render_lighting::{
    lights::{LightClusters, Lights},
    proc_skybox::{ProceduralSkyBox, DI_MAP_SAMPLER},
};
use ard_render_si::{bindings::*, consts::*};

use crate::shadow::{SunShadowsRenderer, SHADOW_SAMPLER};

pub struct ColorPassSets {
    sets: Vec<DescriptorSet>,
}

impl ColorPassSets {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        let sets = (0..frames_in_flight)
            .map(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.color_pass.clone(),
                        debug_name: Some(format!("color_pass_set_{frame_idx}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self { sets }
    }

    pub fn update_sun_shadow_bindings(&mut self, frame: Frame, sun_shadows: &SunShadowsRenderer) {
        let set = &mut self.sets[usize::from(frame)];

        let shadow_cascades_update: [_; MAX_SHADOW_CASCADES + 1] = std::array::from_fn(|i| {
            if i < MAX_SHADOW_CASCADES {
                DescriptorSetUpdate {
                    binding: COLOR_PASS_SET_SHADOW_CASCADES_BINDING,
                    array_element: i,
                    value: DescriptorValue::Texture {
                        texture: sun_shadows
                            .shadow_cascade(i)
                            .unwrap_or(sun_shadows.empty_shadow()),
                        array_element: 0,
                        sampler: SHADOW_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                }
            } else {
                DescriptorSetUpdate {
                    binding: COLOR_PASS_SET_SUN_SHADOW_INFO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: sun_shadows.sun_shadow_info(frame),
                        array_element: 0,
                    },
                }
            }
        });

        set.update(&shadow_cascades_update);
    }

    pub fn update_sky_box_bindings(&mut self, frame: Frame, proc_skybox: &ProceduralSkyBox) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_DI_MAP_BINDING,
                array_element: 0,
                value: DescriptorValue::CubeMap {
                    cube_map: proc_skybox.di_map(),
                    array_element: 0,
                    sampler: DI_MAP_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_ENV_MAP_BINDING,
                array_element: 0,
                value: DescriptorValue::CubeMap {
                    cube_map: proc_skybox.prefiltered_env_map(),
                    array_element: 0,
                    sampler: DI_MAP_SAMPLER,
                    base_mip: 0,
                    mip_count: proc_skybox.prefiltered_env_map().mip_count(),
                },
            },
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_BRDF_LUT_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: proc_skybox.brdf_lut(),
                    array_element: 0,
                    sampler: DI_MAP_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);
    }

    pub fn update_ao_image_binding(&mut self, frame: Frame, ao_image: &Texture) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: COLOR_PASS_SET_AO_IMAGE_BINDING,
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

    pub fn update_light_clusters_binding(&mut self, frame: Frame, clusters: &LightClusters) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: COLOR_PASS_SET_LIGHT_CLUSTERS_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: clusters.clusters(),
                array_element: 0,
            },
        }]);
    }

    pub fn update_lights_binding(&mut self, frame: Frame, lights: &Lights) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_LIGHTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: lights.buffer(),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: lights.global_buffer(),
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
                binding: COLOR_PASS_SET_GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: COLOR_PASS_SET_OBJECT_IDS_BINDING,
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
