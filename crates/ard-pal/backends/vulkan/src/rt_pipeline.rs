use api::rt_pipeline::{
    RayTracingPipelineCreateError, RayTracingPipelineCreateInfo, RayTracingShaderGroup,
    ShaderBindingTableData,
};
use ash::vk;
use crossbeam_channel::Sender;

use crate::{util::garbage_collector::Garbage, VulkanBackend};

pub struct RayTracingPipeline {
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) pipeline: vk::Pipeline,
    group_count: usize,
    garbage: Sender<Garbage>,
}

impl RayTracingPipeline {
    pub(crate) unsafe fn new(
        ctx: &VulkanBackend,
        create_info: RayTracingPipelineCreateInfo<VulkanBackend>,
    ) -> Result<Self, RayTracingPipelineCreateError> {
        let push_constant_ranges = create_info.push_constants_size.map(|size| {
            [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::RAYGEN_KHR
                    | vk::ShaderStageFlags::ANY_HIT_KHR
                    | vk::ShaderStageFlags::CLOSEST_HIT_KHR
                    | vk::ShaderStageFlags::MISS_KHR,
                offset: 0,
                size,
            }]
        });

        // Create the layout
        let mut layouts = Vec::with_capacity(create_info.layouts.len());
        for layout in &create_info.layouts {
            layouts.push(layout.internal().layout);
        }
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .push_constant_ranges(match &push_constant_ranges {
                Some(range) => range,
                None => &[],
            })
            .build();
        let layout = ctx
            .device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        // Create shader stages
        let stages: Vec<_> = create_info
            .stages
            .iter()
            .map(|stage| {
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(crate::util::to_vk_shader_stage(stage.stage))
                    .module(stage.shader.internal().module)
                    .name(c"main")
                    .build()
            })
            .collect();

        // Create groups
        let groups: Vec<_> = create_info
            .groups
            .iter()
            .map(|group| match group {
                RayTracingShaderGroup::RayGeneration(i) => {
                    vk::RayTracingShaderGroupCreateInfoKHR::builder()
                        .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                        .general_shader(*i as u32)
                        .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                        .any_hit_shader(vk::SHADER_UNUSED_KHR)
                        .intersection_shader(vk::SHADER_UNUSED_KHR)
                        .build()
                }
                RayTracingShaderGroup::Miss(i) => vk::RayTracingShaderGroupCreateInfoKHR::builder()
                    .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                    .general_shader(*i as u32)
                    .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                    .any_hit_shader(vk::SHADER_UNUSED_KHR)
                    .intersection_shader(vk::SHADER_UNUSED_KHR)
                    .build(),
                RayTracingShaderGroup::Triangles {
                    closest_hit,
                    any_hit,
                } => vk::RayTracingShaderGroupCreateInfoKHR::builder()
                    .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                    .general_shader(vk::SHADER_UNUSED_KHR)
                    .closest_hit_shader(
                        closest_hit
                            .map(|i| i as u32)
                            .unwrap_or(vk::SHADER_UNUSED_KHR),
                    )
                    .any_hit_shader(any_hit.map(|i| i as u32).unwrap_or(vk::SHADER_UNUSED_KHR))
                    .intersection_shader(vk::SHADER_UNUSED_KHR)
                    .build(),
            })
            .collect();

        let rt_pipeline_create_info = [vk::RayTracingPipelineCreateInfoKHR::builder()
            .stages(&stages)
            .groups(&groups)
            .flags(vk::PipelineCreateFlags::RAY_TRACING_SKIP_AABBS_KHR)
            .max_pipeline_ray_recursion_depth(create_info.max_ray_recursion_depth)
            .layout(layout)
            .build()];

        let pipeline = match ctx.rt_loader.create_ray_tracing_pipelines(
            vk::DeferredOperationKHR::null(),
            vk::PipelineCache::null(),
            &rt_pipeline_create_info,
            None,
        ) {
            Ok(pipeline) => pipeline[0],
            Err(err) => return Err(RayTracingPipelineCreateError::Other(err.to_string())),
        };

        Ok(Self {
            layout,
            pipeline,
            group_count: groups.len(),
            garbage: ctx.garbage.sender(),
        })
    }

    pub(crate) unsafe fn shader_binding_table_data(
        &self,
        ctx: &VulkanBackend,
    ) -> ShaderBindingTableData {
        let raw = ctx
            .rt_loader
            .get_ray_tracing_shader_group_handles(
                self.pipeline,
                0,
                self.group_count as u32,
                ctx.rt_props.shader_group_handle_size as usize * self.group_count,
            )
            .unwrap();

        ShaderBindingTableData {
            raw,
            entry_count: self.group_count,
            entry_size: ctx.rt_props.shader_group_handle_size as u64,
            aligned_size: ctx
                .rt_props
                .shader_group_handle_size
                .next_multiple_of(ctx.rt_props.shader_group_handle_alignment)
                as u64,
            base_alignment: ctx.rt_props.shader_group_base_alignment as u64,
        }
    }
}

impl Drop for RayTracingPipeline {
    fn drop(&mut self) {
        let _ = self.garbage.send(Garbage::Pipeline(self.pipeline));
        let _ = self.garbage.send(Garbage::PipelineLayout(self.layout));
    }
}
