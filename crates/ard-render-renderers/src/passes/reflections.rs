use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, FRAMES_IN_FLIGHT};
use ard_render_lighting::{
    lights::Lights,
    proc_skybox::{ProceduralSkyBox, DI_MAP_SAMPLER},
};
use ard_render_si::bindings::*;

pub struct ReflectionPassSets {
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

impl ReflectionPassSets {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        Self {
            sets: std::array::from_fn(|frame_idx| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.reflection_rt_pass.clone(),
                        debug_name: Some(format!("reflection_pass_set_{frame_idx}")),
                    },
                )
                .unwrap()
            }),
        }
    }

    pub fn update_sky_box_bindings(&mut self, frame: Frame, proc_skybox: &ProceduralSkyBox) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: REFLECTION_RT_PASS_SET_ENV_MAP_BINDING,
            array_element: 0,
            value: DescriptorValue::CubeMap {
                cube_map: proc_skybox.prefiltered_env_map(),
                array_element: 0,
                sampler: DI_MAP_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn update_tlas(&mut self, frame: Frame, tlas: &TopLevelAccelerationStructure) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: REFLECTION_RT_PASS_SET_TLAS_BINDING,
            array_element: 0,
            value: DescriptorValue::TopLevelAccelerationStructure(tlas),
        }]);
    }

    pub fn update_lights_binding(&mut self, frame: Frame, lights: &Lights) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: REFLECTION_RT_PASS_SET_GLOBAL_LIGHTING_INFO_BINDING,
            array_element: 0,
            value: DescriptorValue::UniformBuffer {
                buffer: lights.global_buffer(),
                array_element: 0,
            },
        }]);
    }

    pub fn update_output(&mut self, frame: Frame, tex: &Texture) {
        let set = &mut self.sets[usize::from(frame)];
        set.update(&[DescriptorSetUpdate {
            binding: REFLECTION_RT_PASS_SET_OUTPUT_TEX_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageImage {
                texture: tex,
                array_element: 0,
                mip: 0,
            },
        }]);
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }
}
