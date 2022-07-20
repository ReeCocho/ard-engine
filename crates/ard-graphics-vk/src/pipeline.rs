use crate::prelude::*;
use ash::vk;
use factory::{
    container::{EscapeHandle, ResourceContainer},
    layouts::Layouts,
};
use renderer::{
    forward_plus::{GameRendererGraph, Passes},
    graph::RenderPass,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PipelineType {
    HighZRender,
    ShadowPass,
    DepthPrepass,
    OpaquePass,
    EntityImagePass,
}

#[derive(Clone)]
pub struct Pipeline {
    pub(crate) id: u32,
    pub(crate) inputs: ShaderInputs,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct PipelineInner {
    pub ctx: GraphicsContext,
    pub vertex_layout: VertexLayout,
    pub inputs: ShaderInputs,
    pub pipelines: [vk::Pipeline; 5],
}

impl PipelineApi for Pipeline {}

impl PipelineInner {
    pub unsafe fn new(
        create_info: &PipelineCreateInfo<VkBackend>,
        ctx: &GraphicsContext,
        graph: &GameRendererGraph,
        passes: &Passes,
        layouts: &Layouts,
        shaders: &ResourceContainer<ShaderInner>,
        entity_shader: vk::ShaderModule,
    ) -> Self {
        let vertex = shaders.get(create_info.vertex.id).unwrap();
        let fragment = shaders.get(create_info.fragment.id).unwrap();
        assert_eq!(vertex.inputs, fragment.inputs);

        let highz_render_rp =
            if let RenderPass::Graphics { pass, .. } = graph.get_pass(passes.highz_render) {
                *pass
            } else {
                panic!("High Z render pass not a graphics pass");
            };

        let depth_prepass_rp =
            if let RenderPass::Graphics { pass, .. } = graph.get_pass(passes.depth_prepass) {
                *pass
            } else {
                panic!("Depth prepass not a graphics pass");
            };

        let opaque_pass_rp =
            if let RenderPass::Graphics { pass, .. } = graph.get_pass(passes.opaque_pass) {
                *pass
            } else {
                panic!("Opaque pass not a graphics pass");
            };

        let entity_pass_rp =
            if let RenderPass::Graphics { pass, .. } = graph.get_pass(passes.entity_pass) {
                *pass
            } else {
                panic!("Entity image pass not a graphics pass");
            };

        let entry_point = std::ffi::CString::new("main").unwrap();

        let vertex_layout = vertex.vertex_layout.unwrap();
        let vertex_bindings = bindings_of(&vertex_layout);
        let vertex_attributes = attributes_of(&vertex_layout);
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_bindings)
            .vertex_attribute_descriptions(&vertex_attributes)
            .build();

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false)
            .build();

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0)
            .build();

        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false)
            .build();

        let stencil_state = vk::StencilOpState::builder()
            .fail_op(vk::StencilOp::KEEP)
            .pass_op(vk::StencilOp::KEEP)
            .depth_fail_op(vk::StencilOp::KEEP)
            .compare_op(vk::CompareOp::ALWAYS)
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

        let (depth_prepass, highz_render) = {
            let shader_stages = [vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex.module)
                .name(&entry_point)
                .build()];

            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::LESS)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .build();

            let pipeline_info = [
                vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&shader_stages)
                    .vertex_input_state(&vertex_input_state)
                    .input_assembly_state(&input_assembly)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterizer)
                    .multisample_state(&multisampling)
                    .depth_stencil_state(&depth_stencil)
                    .dynamic_state(&dynamic_state)
                    .layout(layouts.opaque_pipeline_layout)
                    .render_pass(depth_prepass_rp)
                    .subpass(0)
                    .build(),
                vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&shader_stages)
                    .vertex_input_state(&vertex_input_state)
                    .input_assembly_state(&input_assembly)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterizer)
                    .multisample_state(&multisampling)
                    .depth_stencil_state(&depth_stencil)
                    .dynamic_state(&dynamic_state)
                    .layout(layouts.depth_only_pipeline_layout)
                    .render_pass(highz_render_rp)
                    .subpass(0)
                    .build(),
            ];

            let passes = ctx
                .0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create depth pass pipelines");

            (passes[0], passes[1])
        };

        let shadow_pass = {
            let shader_stages = [vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex.module)
                .name(&entry_point)
                .build()];

            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::LESS)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .build();

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
                .depth_clamp_enable(true)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::BACK)
                .front_face(vk::FrontFace::CLOCKWISE)
                .depth_bias_enable(false)
                .depth_bias_constant_factor(0.0)
                .depth_bias_clamp(0.0)
                .depth_bias_slope_factor(0.0)
                .build();

            let pipeline_info = [vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .depth_stencil_state(&depth_stencil)
                .dynamic_state(&dynamic_state)
                .layout(layouts.opaque_pipeline_layout)
                .render_pass(depth_prepass_rp)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create shadow pass pipeline")[0]
        };

        let opaque_pass = {
            let shader_stages = [
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex.module)
                    .name(&entry_point)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment.module)
                    .name(&entry_point)
                    .build(),
            ];

            let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::builder()
                .color_write_mask(
                    vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                )
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ZERO)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .build()];

            let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
                .logic_op_enable(false)
                .logic_op(vk::LogicOp::COPY)
                .attachments(&color_blend_attachment)
                .blend_constants([0.0, 0.0, 0.0, 0.0])
                .build();

            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::EQUAL)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .build();

            let pipeline_info = [vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(layouts.opaque_pipeline_layout)
                .render_pass(opaque_pass_rp)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create opaque pipeline")[0]
        };

        let entity_pass = {
            let shader_stages = [
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex.module)
                    .name(&entry_point)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(entity_shader)
                    .name(&entry_point)
                    .build(),
            ];

            let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::builder()
                .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G)
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ZERO)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .build()];

            let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
                .logic_op_enable(false)
                .logic_op(vk::LogicOp::COPY)
                .attachments(&color_blend_attachment)
                .blend_constants([0.0, 0.0, 0.0, 0.0])
                .build();

            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::EQUAL)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .build();

            let pipeline_info = [vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(layouts.opaque_pipeline_layout)
                .render_pass(entity_pass_rp)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create entity pass pipeline")[0]
        };

        PipelineInner {
            ctx: ctx.clone(),
            pipelines: [
                highz_render,
                shadow_pass,
                depth_prepass,
                opaque_pass,
                entity_pass,
            ],
            vertex_layout,
            inputs: vertex.inputs,
        }
    }
}

impl Drop for PipelineInner {
    fn drop(&mut self) {
        unsafe {
            for pipeline in self.pipelines {
                self.ctx.0.device.destroy_pipeline(pipeline, None);
            }
        }
    }
}

impl PipelineType {
    #[inline]
    pub const fn idx(self) -> usize {
        match self {
            PipelineType::HighZRender => 0,
            PipelineType::ShadowPass => 1,
            PipelineType::DepthPrepass => 2,
            PipelineType::OpaquePass => 3,
            PipelineType::EntityImagePass => 4,
        }
    }
}
