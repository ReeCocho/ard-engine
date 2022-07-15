use crate::{
    alloc::{BufferArray, Image},
    camera::{
        descriptors::DescriptorPool,
        graph::{RenderGraphContext, RenderPass},
        DebugGui, GraphicsContext,
    },
    shader_constants::FRAMES_IN_FLIGHT,
};
use ard_math::{Mat4, Vec2, Vec4};
use ard_render_graph::graph::RenderGraphResources;
use ash::vk;
use bytemuck::{Pod, Zeroable};
use imgui::internal::RawWrapper;

use super::ForwardPlus;

pub(crate) struct GuiRender {
    ctx: GraphicsContext,
    font_image: Image,
    font_view: vk::ImageView,
    font_pool: DescriptorPool,
    sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    font_sampler: vk::Sampler,
    gui_pipeline_layout: vk::PipelineLayout,
    gui_pipeline: vk::Pipeline,
    render_data: Vec<RenderData>,
}

pub(crate) struct DrawCommand {
    scissor: vk::Rect2D,
    buffer_idx: usize,
    vertex_offset: usize,
    vertex_count: usize,
    index_offset: usize,
    index_count: usize,
    texture_id: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct PushConstants {
    pub scale: Vec2,
    pub translate: Vec2,
    pub texture: u32,
}

unsafe impl Pod for PushConstants {}
unsafe impl Zeroable for PushConstants {}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct DrawVert {
    pos: [f32; 2],
    uv: [f32; 2],
    col: [u8; 4],
}

unsafe impl Pod for DrawVert {}
unsafe impl Zeroable for DrawVert {}

pub(crate) struct RenderData {
    buffers: Vec<Buffers>,
    commands: Vec<DrawCommand>,
    view_mat: Mat4,
}

pub(crate) struct Buffers {
    vertex: BufferArray<DrawVert>,
    index: BufferArray<u16>,
}

impl GuiRender {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        textures_layout: vk::DescriptorSetLayout,
        gui_render_pass: vk::RenderPass,
    ) -> (Self, DebugGui) {
        let mut debug_gui = DebugGui::new();

        // Upload font texture
        let (font_image, font_view) = {
            let mut fonts = debug_gui.context.fonts();
            let font_atlas = fonts.build_rgba32_texture();

            let ret = ctx.create_image(
                font_atlas.data,
                1,
                (font_atlas.width, font_atlas.height, 1),
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::SAMPLED,
                vk::ImageCreateFlags::empty(),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            // The texture ID of the font is u32::MAX
            fonts.tex_id = imgui::TextureId::from(u32::MAX as usize);

            ret
        };

        let font_sampler = {
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
                .expect("unable to create font sampler")
        };

        let mut font_pool = {
            let bindings = [
                // Font atlas
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                // Scene view
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, FRAMES_IN_FLIGHT)
        };

        let sets = {
            let mut sets = [vk::DescriptorSet::default(); FRAMES_IN_FLIGHT];

            for set in &mut sets {
                *set = font_pool.allocate();

                let img = [vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(font_view)
                    .sampler(font_sampler)
                    .build()];

                let write = [vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_set(*set)
                    .image_info(&img)
                    .build()];

                ctx.0.device.update_descriptor_sets(&write, &[]);
            }

            sets
        };

        let gui_pipeline_layout = {
            let layouts = [font_pool.layout(), textures_layout];

            let push_ranges = [vk::PushConstantRange::builder()
                .offset(0)
                .size(std::mem::size_of::<PushConstants>() as u32)
                .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                .build()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&push_ranges)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create gui pipeline layout")
        };

        let vertex_shader = {
            let module_create_info = vk::ShaderModuleCreateInfo {
                p_code: GUI_VERT_CODE.as_ptr() as *const u32,
                code_size: GUI_VERT_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&module_create_info, None)
                .expect("unable to compile shader module")
        };

        let fragment_shader = {
            let module_create_info = vk::ShaderModuleCreateInfo {
                p_code: GUI_FRAG_CODE.as_ptr() as *const u32,
                code_size: GUI_FRAG_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&module_create_info, None)
                .expect("unable to compile shader module")
        };

        let gui_pipeline = {
            let entry_point = std::ffi::CString::new("main").unwrap();

            let vertex_bindings = [vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(std::mem::size_of::<DrawVert>() as u32)
                .build()];

            let vertex_attributes = [
                vk::VertexInputAttributeDescription::builder()
                    .binding(0)
                    .location(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(0)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .binding(0)
                    .location(1)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(std::mem::size_of::<Vec2>() as u32)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .binding(0)
                    .location(2)
                    .format(vk::Format::R8G8B8A8_UNORM)
                    .offset(std::mem::size_of::<Vec4>() as u32)
                    .build(),
            ];

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
                    .module(vertex_shader)
                    .name(&entry_point)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_shader)
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
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
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
                .layout(gui_pipeline_layout)
                .render_pass(gui_render_pass)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create gui pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(vertex_shader, None);
        ctx.0.device.destroy_shader_module(fragment_shader, None);

        let mut render_data = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            render_data.push(RenderData {
                buffers: Vec::default(),
                commands: Vec::default(),
                view_mat: Mat4::IDENTITY,
            });
        }

        (
            Self {
                ctx: ctx.clone(),
                font_image,
                font_view,
                font_pool,
                gui_pipeline_layout,
                gui_pipeline,
                render_data,
                font_sampler,
                sets,
            },
            debug_gui,
        )
    }

    pub fn render(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame = ctx.frame();
        let render_data = &mut state.gui.render_data[frame];
        let device = &state.ctx.0.device;

        let canvas_size = resources.get_size_group(state.surface_size_group);

        // Update scene view image
        unsafe {
            let set = state.gui.sets[frame];

            let img = [vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(resources.get_image(state.scene_image).unwrap().1[frame].view)
                .sampler(state.gui.font_sampler)
                .build()];

            let write = [vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_set(set)
                .image_info(&img)
                .build()];

            device.update_descriptor_sets(&write, &[]);
        }

        // Set up state
        unsafe {
            let sets = [
                state.gui.sets[frame],
                state.factory.0.texture_sets.lock().unwrap().get_set(frame),
            ];

            device.cmd_bind_descriptor_sets(
                *commands,
                vk::PipelineBindPoint::GRAPHICS,
                state.gui.gui_pipeline_layout,
                0,
                &sets,
                &[],
            );

            let constants = [PushConstants {
                scale: Vec2::new(
                    2.0 / canvas_size.width as f32,
                    -2.0 / canvas_size.height as f32,
                ),
                translate: Vec2::new(-1.0, 1.0),
                texture: u32::MAX,
            }];

            device.cmd_push_constants(
                *commands,
                state.gui.gui_pipeline_layout,
                vk::ShaderStageFlags::ALL_GRAPHICS,
                0,
                bytemuck::cast_slice(&constants),
            );

            device.cmd_bind_pipeline(
                *commands,
                vk::PipelineBindPoint::GRAPHICS,
                state.gui.gui_pipeline,
            );
        }

        let mut last_buffers = usize::MAX;
        let mut last_texture_id = u32::MAX;

        for command in render_data.commands.drain(..) {
            unsafe {
                // Update vertex/index buffers if needed
                if last_buffers != command.buffer_idx {
                    let buffers = &render_data.buffers[command.buffer_idx];

                    device.cmd_bind_index_buffer(
                        *commands,
                        buffers.index.buffer(),
                        0,
                        vk::IndexType::UINT16,
                    );

                    let vertex_buffer = [buffers.vertex.buffer()];
                    let vertex_offset = [0 as u64];

                    device.cmd_bind_vertex_buffers(*commands, 0, &vertex_buffer, &vertex_offset);

                    last_buffers = command.buffer_idx;
                }

                // Send texture id if needed
                if command.texture_id as u32 != last_texture_id {
                    last_texture_id = command.texture_id as u32;
                    let id = [command.texture_id as u32];
                    device.cmd_push_constants(
                        *commands,
                        state.gui.gui_pipeline_layout,
                        vk::ShaderStageFlags::ALL_GRAPHICS,
                        std::mem::size_of::<Vec2>() as u32 * 2,
                        bytemuck::cast_slice(&id),
                    );
                }

                // Set scissor
                let scissor = [command.scissor];
                device.cmd_set_scissor(*commands, 0, &scissor);

                // Draw triangles
                device.cmd_draw_indexed(
                    *commands,
                    command.index_count as u32,
                    1,
                    command.index_offset as u32,
                    command.vertex_offset as i32,
                    0,
                );
            }
        }
    }

    pub fn prepare(&mut self, frame: usize, draw_data: &imgui::DrawData) {
        let mut render_data = &mut self.render_data[frame];

        // Reset old commands
        render_data.commands.clear();

        // Don't need to draw when minimized
        let fb_width = draw_data.display_size[0] * draw_data.framebuffer_scale[0];
        let fb_height = draw_data.display_size[1] * draw_data.framebuffer_scale[1];
        if !(fb_width > 0.0 && fb_height > 0.0) {
            return;
        }

        // Compute the view matrix
        let left = draw_data.display_pos[0];
        let right = draw_data.display_pos[0] + draw_data.display_size[0];
        let top = draw_data.display_pos[1];
        let bottom = draw_data.display_pos[1] + draw_data.display_size[1];

        render_data.view_mat = Mat4::orthographic_lh(left, right, bottom, top, -1.0, 1.0);

        let clip_off = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;

        for (i, draw_list) in draw_data.draw_lists().enumerate() {
            // Grab data to upload
            let vertex_data = unsafe { draw_list.transmute_vtx_buffer::<DrawVert>() };

            let index_data = draw_list.idx_buffer();

            // Add buffers if needed
            if render_data.buffers.len() <= i {
                let buffers = unsafe {
                    let vertex = BufferArray::new(
                        &self.ctx,
                        vertex_data.len(),
                        vk::BufferUsageFlags::VERTEX_BUFFER,
                    );
                    let index = BufferArray::new(
                        &self.ctx,
                        index_data.len(),
                        vk::BufferUsageFlags::INDEX_BUFFER,
                    );

                    Buffers { vertex, index }
                };

                render_data.buffers.push(buffers);
            }

            // Upload data
            let buffers = &mut render_data.buffers[i];
            unsafe {
                // Resize buffers if needed
                if buffers.vertex.cap() < vertex_data.len() {
                    buffers.vertex.expand(vertex_data.len());
                }

                if buffers.index.cap() < index_data.len() {
                    buffers.index.expand(index_data.len());
                }

                buffers.vertex.write_slice(0, vertex_data);
                buffers.index.write_slice(0, index_data);
            }

            for cmd in draw_list.commands() {
                match cmd {
                    imgui::DrawCmd::Elements {
                        count,
                        cmd_params:
                            imgui::DrawCmdParams {
                                clip_rect,
                                texture_id,
                                vtx_offset,
                                idx_offset,
                                ..
                            },
                    } => {
                        let clip_rect = [
                            (clip_rect[0] - clip_off[0]) * clip_scale[0],
                            (clip_rect[1] - clip_off[1]) * clip_scale[1],
                            (clip_rect[2] - clip_off[0]) * clip_scale[0],
                            (clip_rect[3] - clip_off[1]) * clip_scale[1],
                        ];

                        if !(clip_rect[0] < fb_width
                            && clip_rect[1] < fb_height
                            && clip_rect[2] >= 0.0
                            && clip_rect[3] >= 0.0)
                        {
                            continue;
                        }

                        let clip_rect = [
                            clip_rect[0] as i32,
                            clip_rect[1] as i32,
                            clip_rect[2] as i32,
                            clip_rect[3] as i32,
                        ];

                        render_data.commands.push(DrawCommand {
                            scissor: vk::Rect2D {
                                offset: vk::Offset2D {
                                    x: clip_rect[0].max(0),
                                    y: clip_rect[1].max(1),
                                },
                                extent: vk::Extent2D {
                                    width: (clip_rect[2] - clip_rect[0]) as u32,
                                    height: (clip_rect[3] - clip_rect[1]) as u32,
                                },
                            },
                            buffer_idx: i,
                            vertex_offset: vtx_offset,
                            vertex_count: vertex_data.len() - vtx_offset,
                            index_offset: idx_offset,
                            index_count: count,
                            texture_id: texture_id.id(),
                        });
                    }
                    imgui::DrawCmd::ResetRenderState => {}
                    imgui::DrawCmd::RawCallback { callback, raw_cmd } => unsafe {
                        callback(draw_list.raw(), raw_cmd)
                    },
                }
            }
        }
    }
}

impl Drop for GuiRender {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.device.destroy_image_view(self.font_view, None);
            self.ctx.0.device.destroy_sampler(self.font_sampler, None);
            self.ctx
                .0
                .device
                .destroy_pipeline_layout(self.gui_pipeline_layout, None);
            self.ctx.0.device.destroy_pipeline(self.gui_pipeline, None);
        }
    }
}

const GUI_FRAG_CODE: &[u8] = include_bytes!("../gui.frag.spv");

const GUI_VERT_CODE: &[u8] = include_bytes!("../gui.vert.spv");
