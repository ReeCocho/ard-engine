use api::graphics_pipeline::{GraphicsPipelineCreateInfo, ShaderStages};
use ash::vk;
use crossbeam_channel::Sender;
use std::ffi::CString;

use crate::{
    render_pass::VkRenderPass,
    util::{garbage_collector::Garbage, pipeline_cache::PipelineCache},
};

pub struct GraphicsPipeline {
    descriptor: GraphicsPipelineCreateInfo<crate::VulkanBackend>,
    layout: vk::PipelineLayout,
    garbage: Sender<Garbage>,
}

impl GraphicsPipeline {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        garbage: Sender<Garbage>,
        descriptor: GraphicsPipelineCreateInfo<crate::VulkanBackend>,
    ) -> Self {
        let push_constant_ranges = descriptor.push_constants_size.map(|size| {
            [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::ALL_GRAPHICS
                    | vk::ShaderStageFlags::MESH_EXT
                    | vk::ShaderStageFlags::TASK_EXT
                    | vk::ShaderStageFlags::RAYGEN_KHR
                    | vk::ShaderStageFlags::MISS_KHR
                    | vk::ShaderStageFlags::ANY_HIT_KHR
                    | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                offset: 0,
                size,
            }]
        });

        // Create the layout
        let mut layouts = Vec::with_capacity(descriptor.layouts.len());
        for layout in &descriptor.layouts {
            layouts.push(layout.internal().layout);
        }
        let layout_create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(match &push_constant_ranges {
                Some(range) => range,
                None => &[],
            });
        let layout = device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        Self {
            descriptor,
            layout,
            garbage,
        }
    }

    #[inline(always)]
    pub(crate) fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    /// Retrieves a pipeline and layout, or creates a new one if needed.
    pub(crate) unsafe fn get(
        &self,
        device: &ash::Device,
        pipelines: &mut PipelineCache,
        debug: Option<&ash::ext::debug_utils::Device>,
        render_pass: VkRenderPass,
    ) -> vk::Pipeline {
        if let Some(pipeline) = pipelines.get(self.layout, render_pass.pass) {
            return pipeline;
        }

        // Need to create a new pipeline
        let mut bindings = Vec::with_capacity(self.descriptor.vertex_input.bindings.len());
        for binding in &self.descriptor.vertex_input.bindings {
            bindings.push(vk::VertexInputBindingDescription {
                binding: binding.binding,
                stride: binding.stride,
                input_rate: crate::util::to_vk_vertex_rate(binding.input_rate),
            });
        }

        let mut attributes = Vec::with_capacity(self.descriptor.vertex_input.attributes.len());
        for attribute in &self.descriptor.vertex_input.attributes {
            attributes.push(vk::VertexInputAttributeDescription {
                location: attribute.location,
                binding: attribute.binding,
                format: crate::util::to_vk_format(attribute.format),
                offset: attribute.offset,
            });
        }

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(
            crate::util::to_vk_topology(self.descriptor.vertex_input.topology),
        );

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .cull_mode(crate::util::to_vk_cull_mode(
                self.descriptor.rasterization.cull_mode,
            ))
            .front_face(crate::util::to_vk_front_face(
                self.descriptor.rasterization.front_face,
            ))
            .polygon_mode(crate::util::to_vk_polygon_mode(
                self.descriptor.rasterization.polygon_mode,
            ))
            .depth_clamp_enable(match &self.descriptor.depth_stencil {
                Some(depth_stencil) => depth_stencil.depth_clamp,
                None => false,
            })
            .line_width(1.0);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(render_pass.samples)
            .alpha_to_coverage_enable(if render_pass.samples == vk::SampleCountFlags::TYPE_1 {
                    false
                } else {
                    self.descriptor.rasterization.alpha_to_coverage
                })
            .alpha_to_one_enable(false);

        // NOTE: For the viewport and scissor the width and height doesn't really matter
        // because the dynamic stage can change them.
        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            min_depth: 0.0,
            max_depth: 1.0,
        }];

        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
        }];

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let spec_map_entries = [
            vk::SpecializationMapEntry::default()
                .constant_id(0)
                .offset(0)
                .size(std::mem::size_of::<u32>()),
            vk::SpecializationMapEntry::default()
                .constant_id(1)
                .offset(std::mem::size_of::<u32>() as u32)
                .size(std::mem::size_of::<u32>()),
            vk::SpecializationMapEntry::default()
                .constant_id(2)
                .offset(2 * std::mem::size_of::<u32>() as u32)
                .size(std::mem::size_of::<u32>()),
        ];
        let mesh_spec_map_values;
        let task_spec_map_values;
        let mesh_spec;
        let task_spec;

        let entry_point = std::ffi::CString::new("main").unwrap();
        let shader_stages = match &self.descriptor.stages {
            ShaderStages::Traditional { vertex, fragment } => {
                let mut shader_stages = Vec::with_capacity(2);
                shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex.internal().module)
                        .name(&entry_point),
                );
                if let Some(stage) = fragment {
                    shader_stages.push(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::FRAGMENT)
                            .module(stage.internal().module)
                            .name(&entry_point),
                    );
                }
                shader_stages
            }
            ShaderStages::MeshShading {
                task,
                mesh,
                fragment,
            } => {
                let mut shader_stages = Vec::with_capacity(3);

                mesh_spec_map_values = vec![
                    mesh.work_group_size.0,
                    mesh.work_group_size.1,
                    mesh.work_group_size.2,
                ];

                mesh_spec = vk::SpecializationInfo::default()
                    .map_entries(&spec_map_entries)
                    .data(bytemuck::cast_slice(&mesh_spec_map_values));

                shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::MESH_EXT)
                        .module(mesh.shader.internal().module)
                        .name(&entry_point)
                        .specialization_info(&mesh_spec),
                );

                if let Some(stage) = task {
                    task_spec_map_values = vec![
                        stage.work_group_size.0,
                        stage.work_group_size.1,
                        stage.work_group_size.2,
                    ];

                    task_spec = vk::SpecializationInfo::default()
                        .map_entries(&spec_map_entries)
                        .data(bytemuck::cast_slice(&task_spec_map_values));

                    shader_stages.push(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::TASK_EXT)
                            .module(stage.shader.internal().module)
                            .name(&entry_point)
                            .specialization_info(&task_spec),
                    );
                }
                if let Some(stage) = fragment {
                    shader_stages.push(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::FRAGMENT)
                            .module(stage.internal().module)
                            .name(&entry_point),
                    );
                }
                shader_stages
            }
        };

        let depth_stencil = match &self.descriptor.depth_stencil {
            Some(depth_stencil) => vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(depth_stencil.depth_test)
                .depth_write_enable(depth_stencil.depth_write && !render_pass.read_only_depth)
                .depth_compare_op(crate::util::to_vk_compare_op(depth_stencil.depth_compare))
                .min_depth_bounds(depth_stencil.min_depth)
                .max_depth_bounds(depth_stencil.max_depth),
            None => vk::PipelineDepthStencilStateCreateInfo::default(),
        };

        let mut attachments = Vec::with_capacity(self.descriptor.color_blend.attachments.len());
        let color_blend = if self.descriptor.color_blend.attachments.is_empty() {
            vk::PipelineColorBlendStateCreateInfo::default()
        } else {
            for attachment in &self.descriptor.color_blend.attachments {
                attachments.push(
                    vk::PipelineColorBlendAttachmentState::default()
                        .color_write_mask(crate::util::to_vk_color_components(
                            attachment.write_mask,
                        ))
                        .blend_enable(attachment.blend)
                        .src_color_blend_factor(crate::util::to_vk_blend_factor(
                            attachment.src_color_blend_factor,
                        ))
                        .dst_color_blend_factor(crate::util::to_vk_blend_factor(
                            attachment.dst_color_blend_factor,
                        ))
                        .color_blend_op(crate::util::to_vk_blend_op(attachment.color_blend_op))
                        .src_alpha_blend_factor(crate::util::to_vk_blend_factor(
                            attachment.src_alpha_blend_factor,
                        ))
                        .dst_alpha_blend_factor(crate::util::to_vk_blend_factor(
                            attachment.dst_alpha_blend_factor,
                        ))
                        .alpha_blend_op(crate::util::to_vk_blend_op(attachment.alpha_blend_op)),
                );
            }
            vk::PipelineColorBlendStateCreateInfo::default()
                .attachments(&attachments)
                .logic_op(vk::LogicOp::COPY)
                .logic_op_enable(false)
                .blend_constants([0.0, 0.0, 0.0, 0.0])
        };

        let create_info = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .dynamic_state(&dynamic_state)
            .layout(self.layout)
            .color_blend_state(&color_blend)
            .render_pass(render_pass.pass)
            .subpass(0)];

        let pipeline = device
            .create_graphics_pipelines(vk::PipelineCache::null(), &create_info, None)
            .unwrap()[0];

        // Name the pipeline if requested
        if let Some(name) = &self.descriptor.debug_name {
            if let Some(debug) = debug {
                let name = CString::new(format!(
                    "{}_{}",
                    name.as_str(),
                    pipelines.count(self.layout)
                ))
                .unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(pipeline)
                    .object_name(&name);

                debug.set_debug_utils_object_name(&name_info).unwrap();
            }
        }

        pipelines.insert(self.layout, render_pass.pass, pipeline);
        pipeline
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        let _ = self.garbage.send(Garbage::PipelineLayout(self.layout));
    }
}
