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
        let layout_create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(match &push_constant_ranges {
                Some(range) => range,
                None => &[],
            });
        let layout = ctx
            .device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        // Create shader stages
        let stages: Vec<_> = create_info
            .stages
            .iter()
            .map(|stage| {
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(crate::util::to_vk_shader_stage(stage.stage))
                    .module(stage.shader.internal().module)
                    .name(c"main")
            })
            .collect();

        // Create groups
        let groups: Vec<_> = create_info
            .groups
            .iter()
            .map(|group| match group {
                RayTracingShaderGroup::RayGeneration(i) => {
                    vk::RayTracingShaderGroupCreateInfoKHR::default()
                        .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                        .general_shader(*i as u32)
                        .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                        .any_hit_shader(vk::SHADER_UNUSED_KHR)
                        .intersection_shader(vk::SHADER_UNUSED_KHR)
                }
                RayTracingShaderGroup::Miss(i) => vk::RayTracingShaderGroupCreateInfoKHR::default()
                    .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                    .general_shader(*i as u32)
                    .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                    .any_hit_shader(vk::SHADER_UNUSED_KHR)
                    .intersection_shader(vk::SHADER_UNUSED_KHR),
                RayTracingShaderGroup::Triangles {
                    closest_hit,
                    any_hit,
                } => vk::RayTracingShaderGroupCreateInfoKHR::default()
                    .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                    .general_shader(vk::SHADER_UNUSED_KHR)
                    .closest_hit_shader(
                        closest_hit
                            .map(|i| i as u32)
                            .unwrap_or(vk::SHADER_UNUSED_KHR),
                    )
                    .any_hit_shader(any_hit.map(|i| i as u32).unwrap_or(vk::SHADER_UNUSED_KHR))
                    .intersection_shader(vk::SHADER_UNUSED_KHR),
            })
            .collect();

        let libraries: Vec<_> = create_info
            .libraries
            .iter()
            .map(|pip| pip.internal().pipeline)
            .collect();

        let rt_libraries = vk::PipelineLibraryCreateInfoKHR::default().libraries(&libraries);

        let rt_interface = match create_info.library_info {
            Some(info) => vk::RayTracingPipelineInterfaceCreateInfoKHR::default()
                .max_pipeline_ray_hit_attribute_size(info.max_ray_hit_attribute_size)
                .max_pipeline_ray_payload_size(info.max_ray_payload_size),
            None => vk::RayTracingPipelineInterfaceCreateInfoKHR::default(),
        };

        let mut rt_pipeline_create_info = [vk::RayTracingPipelineCreateInfoKHR::default()
            .library_info(&rt_libraries)
            .stages(&stages)
            .groups(&groups)
            .flags(vk::PipelineCreateFlags::RAY_TRACING_SKIP_AABBS_KHR)
            .max_pipeline_ray_recursion_depth(create_info.max_ray_recursion_depth)
            .layout(layout)];

        if let Some(info) = &create_info.library_info {
            if info.is_library {
                rt_pipeline_create_info[0].flags |= vk::PipelineCreateFlags::LIBRARY_KHR;
            }
            rt_pipeline_create_info[0].p_library_interface = &rt_interface;
        }

        let pipeline = match ctx.rt_loader.create_ray_tracing_pipelines(
            vk::DeferredOperationKHR::null(),
            vk::PipelineCache::null(),
            &rt_pipeline_create_info,
            None,
        ) {
            Ok(pipeline) => pipeline[0],
            // TODO: Destroy pipelines here
            Err((_, err)) => return Err(RayTracingPipelineCreateError::Other(err.to_string())),
        };

        let mut group_count = groups.len();
        for lib in &create_info.libraries {
            group_count += lib.internal().group_count;
        }

        Ok(Self {
            layout,
            pipeline,
            group_count,
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
                ctx.properties.shader_group_handle_size as usize * self.group_count,
            )
            .unwrap();

        ShaderBindingTableData {
            raw,
            entry_count: self.group_count,
            entry_size: ctx.properties.shader_group_handle_size as u64,
            aligned_size: ctx
                .properties
                .shader_group_handle_size
                .next_multiple_of(ctx.properties.shader_group_handle_alignment)
                as u64,
            base_alignment: ctx.properties.shader_group_base_alignment as u64,
        }
    }
}

impl Drop for RayTracingPipeline {
    fn drop(&mut self) {
        let _ = self.garbage.send(Garbage::Pipeline(self.pipeline));
        let _ = self.garbage.send(Garbage::PipelineLayout(self.layout));
    }
}
