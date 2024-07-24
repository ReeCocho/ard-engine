use ard_pal::prelude::*;
use ard_render_base::{Frame, FRAMES_IN_FLIGHT};
use ard_render_image_effects::ao::AO_SAMPLER;
use ard_render_lighting::{
    lights::{LightClusters, Lights},
    proc_skybox::{ProceduralSkyBox, DI_MAP_SAMPLER},
};
use ard_render_si::{bindings::*, consts::*};

use crate::{
    highz::HzbImage,
    ids::RenderIds,
    shadow::{SunShadowsRenderer, SHADOW_SAMPLER},
};

pub struct TransparentPassSets {
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

impl TransparentPassSets {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        Self {
            sets: std::array::from_fn(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.transparent_pass.clone(),
                        debug_name: Some(format!("transparent_pass_set_{frame_idx}")),
                    },
                )
                .unwrap()
            }),
        }
    }

    pub fn update_sun_shadow_bindings(&mut self, frame: Frame, sun_shadows: &SunShadowsRenderer) {
        let set = &mut self.sets[usize::from(frame)];

        let shadow_cascades_update: [_; MAX_SHADOW_CASCADES + 1] = std::array::from_fn(|i| {
            if i < MAX_SHADOW_CASCADES {
                DescriptorSetUpdate {
                    binding: TRANSPARENT_PASS_SET_SHADOW_CASCADES_BINDING,
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
                    binding: TRANSPARENT_PASS_SET_SUN_SHADOW_INFO_BINDING,
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
                binding: TRANSPARENT_PASS_SET_DI_MAP_BINDING,
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
                binding: TRANSPARENT_PASS_SET_ENV_MAP_BINDING,
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
                binding: TRANSPARENT_PASS_SET_BRDF_LUT_BINDING,
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
            binding: TRANSPARENT_PASS_SET_AO_IMAGE_BINDING,
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
            binding: TRANSPARENT_PASS_SET_LIGHT_CLUSTERS_BINDING,
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
                binding: TRANSPARENT_PASS_SET_LIGHTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: lights.buffer(),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: TRANSPARENT_PASS_SET_GLOBAL_LIGHTING_INFO_BINDING,
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
        object_ids: &RenderIds,
    ) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[
            DescriptorSetUpdate {
                binding: TRANSPARENT_PASS_SET_GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_data,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: TRANSPARENT_PASS_SET_INPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids.input(),
                    array_element: usize::from(frame),
                },
            },
            DescriptorSetUpdate {
                binding: TRANSPARENT_PASS_SET_OUTPUT_IDS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: object_ids.output(),
                    array_element: 0,
                },
            },
        ]);
    }

    pub fn update_hzb_binding(&mut self, frame: Frame, image: &HzbImage) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: TRANSPARENT_PASS_SET_HZB_IMAGE_BINDING,
            array_element: 0,
            value: image.descriptor_value(),
        }]);
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }
}
