use api::graphics_pipeline::GraphicsPipelineCreateInfo;
use ash::vk::{self, Handle};
use crossbeam_channel::Sender;
use std::ffi::CString;

use crate::util::{garbage_collector::Garbage, pipeline_cache::PipelineCache};

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
        // Create the layout
        let mut layouts = Vec::with_capacity(descriptor.layouts.len());
        for layout in &descriptor.layouts {
            layouts.push(layout.internal().layout);
        }
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .build();
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
        debug: Option<&ash::extensions::ext::DebugUtils>,
        render_pass: vk::RenderPass,
    ) -> vk::Pipeline {
        if let Some(pipeline) = pipelines.get(self.layout, render_pass) {
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
                format: crate::util::to_vk_vertex_format(attribute.format),
                offset: attribute.offset,
            });
        }

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes)
            .build();

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(crate::util::to_vk_topology(
                self.descriptor.vertex_input.topology,
            ))
            .build();

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
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
            .line_width(1.0)
            .build();

        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false)
            .build();

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

        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors)
            .build();

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(&dynamic_states)
            .build();

        let entry_point = std::ffi::CString::new("main").unwrap();
        let mut shader_stages = Vec::with_capacity(2);
        shader_stages.push(
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(self.descriptor.stages.vertex.internal().module)
                .name(&entry_point)
                .build(),
        );
        if let Some(stage) = &self.descriptor.stages.fragment {
            shader_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(stage.internal().module)
                    .name(&entry_point)
                    .build(),
            );
        }

        let depth_stencil = match &self.descriptor.depth_stencil {
            Some(depth_stencil) => vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(depth_stencil.depth_test)
                .depth_write_enable(depth_stencil.depth_write)
                .depth_compare_op(crate::util::to_vk_compare_op(depth_stencil.depth_compare))
                .min_depth_bounds(depth_stencil.min_depth)
                .max_depth_bounds(depth_stencil.max_depth)
                .build(),
            None => vk::PipelineDepthStencilStateCreateInfo::default(),
        };

        let (color_blend, _attachments) = match &self.descriptor.color_blend {
            Some(color_blend) => {
                let mut attachments = Vec::with_capacity(color_blend.attachments.len());
                for attachment in &color_blend.attachments {
                    attachments.push(
                        vk::PipelineColorBlendAttachmentState::builder()
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
                            .color_blend_op(vk::BlendOp::ADD)
                            .src_alpha_blend_factor(crate::util::to_vk_blend_factor(
                                attachment.src_alpha_blend_factor,
                            ))
                            .dst_alpha_blend_factor(crate::util::to_vk_blend_factor(
                                attachment.dst_alpha_blend_factor,
                            ))
                            .alpha_blend_op(vk::BlendOp::ADD)
                            .build(),
                    );
                }
                let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                    .attachments(&attachments)
                    .logic_op(vk::LogicOp::COPY)
                    .logic_op_enable(false)
                    .blend_constants([0.0, 0.0, 0.0, 0.0])
                    .build();

                (color_blend, attachments)
            }
            None => (
                vk::PipelineColorBlendStateCreateInfo::default(),
                Vec::default(),
            ),
        };

        let create_info = [vk::GraphicsPipelineCreateInfo::builder()
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
            .render_pass(render_pass)
            .subpass(0)
            .build()];

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
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::PIPELINE)
                    .object_handle(pipeline.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .debug_utils_set_object_name(device.handle(), &name_info)
                    .unwrap();
            }
        }

        pipelines.insert(self.layout, render_pass, pipeline);
        pipeline
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        self.garbage
            .send(Garbage::PipelineLayout(self.layout))
            .unwrap();
    }
}
