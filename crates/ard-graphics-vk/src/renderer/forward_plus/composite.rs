use ard_render_graph::graph::RenderGraphResources;
use ash::vk;

use crate::{
    camera::{
        descriptors::DescriptorPool,
        graph::{RenderGraphContext, RenderPass},
        GraphicsContext,
    },
    shader_constants::FRAMES_IN_FLIGHT,
};

use super::ForwardPlus;

pub(crate) struct Composite {
    ctx: GraphicsContext,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    pool: DescriptorPool,
    sampler: vk::Sampler,
    composite_data: [CompositeData; FRAMES_IN_FLIGHT],
    pub render_scene: bool,
}

#[derive(Default, Copy, Clone)]
struct CompositeData {
    last_scene_image: vk::Image,
    scene_set: vk::DescriptorSet,
}

impl Composite {
    pub unsafe fn new(ctx: &GraphicsContext, pass: vk::RenderPass, render_scene: bool) -> Self {
        let mut pool = {
            let bindings = [vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build()];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, FRAMES_IN_FLIGHT)
        };

        let pipeline_layout = {
            let layouts = [pool.layout()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create composite pipeline layout")
        };

        let vert_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: COMPOSITE_VERT_SHADER_CODE.as_ptr() as *const u32,
                code_size: COMPOSITE_VERT_SHADER_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create quad vertex shader module")
        };

        let frag_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: COMPOSITE_FRAG_SHADER_CODE.as_ptr() as *const u32,
                code_size: COMPOSITE_FRAG_SHADER_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create composite fragment shader module")
        };

        let pipeline = {
            let entry_name = std::ffi::CString::new("main").unwrap();

            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder().build();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false)
                .build();

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::NONE)
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

            let shader_stages = [
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert_module)
                    .name(&entry_name)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag_module)
                    .name(&entry_name)
                    .build(),
            ];

            let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::builder()
                .color_write_mask(
                    vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                )
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
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
                .depth_test_enable(false)
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
                .layout(pipeline_layout)
                .render_pass(pass)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create composite pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(vert_module, None);
        ctx.0.device.destroy_shader_module(frag_module, None);

        let sampler = {
            let create_info = vk::SamplerCreateInfo::builder()
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0)
                .max_lod(0.0)
                .anisotropy_enable(false)
                .build();

            ctx.0
                .device
                .create_sampler(&create_info, None)
                .expect("unable to create composite sampler")
        };

        let mut composite_data = [CompositeData::default(); FRAMES_IN_FLIGHT];
        for data in &mut composite_data {
            data.scene_set = pool.allocate();
        }

        Self {
            ctx: ctx.clone(),
            pipeline,
            pipeline_layout,
            pool,
            sampler,
            composite_data,
            render_scene,
        }
    }

    pub fn compose(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame = ctx.frame();
        let device = state.ctx.0.device.as_ref();
        let mut data = &mut state.composite.composite_data[frame];

        // Rebind images if they are different from last time
        if state.composite.render_scene {
            let scene_image = &resources.get_image(state.scene_image).unwrap().1[frame];
            if scene_image.image.image() != data.last_scene_image {
                data.last_scene_image = scene_image.image.image();

                let src_img = [vk::DescriptorImageInfo::builder()
                    .image_view(scene_image.view)
                    .sampler(state.composite.sampler)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .build()];

                let writes = [vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_set(data.scene_set)
                    .image_info(&src_img)
                    .build()];

                unsafe {
                    device.update_descriptor_sets(&writes, &[]);
                }
            }
        }

        // Perform composition
        unsafe {
            device.cmd_bind_pipeline(
                *commands,
                vk::PipelineBindPoint::GRAPHICS,
                state.composite.pipeline,
            );

            if state.composite.render_scene {
                let sets = [data.scene_set];
                device.cmd_bind_descriptor_sets(
                    *commands,
                    vk::PipelineBindPoint::GRAPHICS,
                    state.composite.pipeline_layout,
                    0,
                    &sets,
                    &[],
                );

                device.cmd_draw(*commands, 3, 1, 0, 0);
            }
        }
    }
}

impl Drop for Composite {
    fn drop(&mut self) {
        unsafe {
            let device = self.ctx.0.device.as_ref();

            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_sampler(self.sampler, None);
        }
    }
}

const COMPOSITE_FRAG_SHADER_CODE: &[u8] = include_bytes!("../composite.frag.spv");
const COMPOSITE_VERT_SHADER_CODE: &[u8] = include_bytes!("../quad.vert.spv");
