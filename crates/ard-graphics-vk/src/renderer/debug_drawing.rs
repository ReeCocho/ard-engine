use ard_math::{Mat4, Vec3, Vec4};
use ash::vk::{self, VertexInputBindingDescription};
use std::sync::{Arc, Mutex};

use crate::{alloc::BufferArray, prelude::*};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;

use crate::VkBackend;

use crate::shader_constants::FRAMES_IN_FLIGHT;

use super::forward_plus::mesh_passes::mesh_pass::MeshPassCameraInfo;

const DEFAULT_DEBUG_BUFFER_CAP: usize = 1;

#[derive(Resource, Clone)]
pub struct DebugDrawing(pub(crate) Arc<DebugDrawingInner>);

pub(crate) struct DebugDrawingInner {
    ctx: GraphicsContext,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    /// List of debug objects paired with their color.
    draws: Mutex<Vec<(DebugObject, Vec3)>>,
    frame_data: Mutex<Vec<FrameData>>,
}

struct FrameData {
    position_buffer: BufferArray<Vec4>,
    color_buffer: BufferArray<Vec4>,
}

enum DebugObject {
    Line { a: Vec3, b: Vec3 },
    Sphere { center: Vec3, radius: f32 },
    Frustum(CameraDescriptor),
    RectPrism { half_extents: Vec3, transform: Mat4 },
}

impl DebugDrawing {
    pub(crate) unsafe fn new(
        ctx: &GraphicsContext,
        camera_layout: vk::DescriptorSetLayout,
        opaque_render_pass: vk::RenderPass,
    ) -> Self {
        Self(Arc::new(DebugDrawingInner::new(
            ctx,
            camera_layout,
            opaque_render_pass,
        )))
    }
}

impl DebugDrawingInner {
    pub(crate) unsafe fn new(
        ctx: &GraphicsContext,
        camera_layout: vk::DescriptorSetLayout,
        opaque_render_pass: vk::RenderPass,
    ) -> Self {
        let mut frames = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            frames.push(FrameData::new(ctx));
        }

        let pipeline_layout = {
            let layouts = [camera_layout];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create debug pipeline layout")
        };

        let vert_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: DEBUG_DRAW_VERT_CODE.as_ptr() as *const u32,
                code_size: DEBUG_DRAW_VERT_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to debug drawing vertex shader module")
        };

        let frag_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: DEBUG_DRAW_FRAG_CODE.as_ptr() as *const u32,
                code_size: DEBUG_DRAW_FRAG_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to debug drawing fragment shader module")
        };

        let pipeline = {
            let entry_point = std::ffi::CString::new("main").unwrap();

            let vertex_bindings = [
                VertexInputBindingDescription::builder()
                    .binding(0)
                    .input_rate(vk::VertexInputRate::VERTEX)
                    .stride(std::mem::size_of::<Vec4>() as u32)
                    .build(),
                VertexInputBindingDescription::builder()
                    .binding(1)
                    .input_rate(vk::VertexInputRate::VERTEX)
                    .stride(std::mem::size_of::<Vec4>() as u32)
                    .build(),
            ];

            let vertex_attributes = [
                vk::VertexInputAttributeDescription::builder()
                    .binding(0)
                    .location(0)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(0)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .binding(1)
                    .location(1)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(0)
                    .build(),
            ];

            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
                .vertex_binding_descriptions(&vertex_bindings)
                .vertex_attribute_descriptions(&vertex_attributes)
                .build();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                .topology(vk::PrimitiveTopology::LINE_LIST)
                .primitive_restart_enable(false)
                .build();

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::LINE)
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
                    .name(&entry_point)
                    .build(),
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag_module)
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
                .depth_test_enable(false)
                .depth_write_enable(false)
                .front(stencil_state)
                .back(stencil_state)
                .depth_compare_op(vk::CompareOp::ALWAYS)
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
                .render_pass(opaque_render_pass)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create debug drawing pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(vert_module, None);
        ctx.0.device.destroy_shader_module(frag_module, None);

        Self {
            ctx: ctx.clone(),
            frame_data: Mutex::new(frames),
            pipeline,
            pipeline_layout,
            draws: Mutex::new(Vec::default()),
        }
    }
}

impl DebugDrawingInner {
    pub(crate) unsafe fn render(
        &self,
        commands: vk::CommandBuffer,
        frame_idx: usize,
        camera: &MeshPassCameraInfo,
        screen_size: (u32, u32),
    ) {
        let device = &self.ctx.0.device;
        let mut draws = self.draws.lock().expect("mutex poisoned");
        let mut frames = self.frame_data.lock().expect("mutex poisoned");
        let frame = &mut frames[frame_idx];

        // Don't need to render if we have no draws
        if draws.is_empty() {
            return;
        }

        // Do a first pass to determine how big we need our buffers to be
        let mut point_count = 0;
        for (draw, _) in draws.iter() {
            point_count += match draw {
                // Line only needs two points
                DebugObject::Line { .. } => 2,
                DebugObject::Sphere { .. } => todo!(),
                // Prism is made up of 12 lines (24 points)
                DebugObject::Frustum(_) => 24,
                // Prism is made up of 12 lines (24 points)
                DebugObject::RectPrism { .. } => 24,
            };
        }

        // Expand our buffers
        frame.position_buffer.expand(point_count);
        frame.color_buffer.expand(point_count);

        // Second pass to write vertices into the buffers
        let mut offset = 0;
        for (draw, color) in draws.drain(..) {
            let color = Vec4::new(color.x, color.y, color.z, 1.0);

            match draw {
                DebugObject::Line { a, b } => {
                    frame.position_buffer.write_slice(
                        offset,
                        &[Vec4::new(a.x, a.y, a.z, 1.0), Vec4::new(b.x, b.y, b.z, 1.0)],
                    );
                    frame.color_buffer.write_slice(offset, &[color, color]);
                    offset += 2;
                }
                DebugObject::Sphere { .. } => todo!(),
                DebugObject::Frustum(descriptor) => {
                    let mut positions = [
                        // Vertical lines
                        Vec4::new(-1.0, -1.0, 0.0, 1.0),
                        Vec4::new(-1.0, 1.0, 0.0, 1.0),
                        Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        Vec4::new(-1.0, 1.0, 1.0, 1.0),
                        Vec4::new(1.0, -1.0, 0.0, 1.0),
                        Vec4::new(1.0, 1.0, 0.0, 1.0),
                        Vec4::new(1.0, -1.0, 1.0, 1.0),
                        Vec4::new(1.0, 1.0, 1.0, 1.0),
                        // Depth lines
                        Vec4::new(-1.0, -1.0, 0.0, 1.0),
                        Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        Vec4::new(1.0, -1.0, 0.0, 1.0),
                        Vec4::new(1.0, -1.0, 1.0, 1.0),
                        Vec4::new(-1.0, 1.0, 0.0, 1.0),
                        Vec4::new(-1.0, 1.0, 1.0, 1.0),
                        Vec4::new(1.0, 1.0, 0.0, 1.0),
                        Vec4::new(1.0, 1.0, 1.0, 1.0),
                        // Horizontal lines
                        Vec4::new(1.0, -1.0, 0.0, 1.0),
                        Vec4::new(-1.0, -1.0, 0.0, 1.0),
                        Vec4::new(1.0, 1.0, 0.0, 1.0),
                        Vec4::new(-1.0, 1.0, 0.0, 1.0),
                        Vec4::new(1.0, -1.0, 1.0, 1.0),
                        Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        Vec4::new(1.0, 1.0, 1.0, 1.0),
                        Vec4::new(-1.0, 1.0, 1.0, 1.0),
                    ];

                    // Convert lines from clip space to world space
                    let (width, height) = (screen_size.0 as f32, screen_size.1 as f32);
                    let vp_inv = CameraUBO::new(&descriptor, width, height).vp.inverse();
                    for position in &mut positions {
                        *position = vp_inv * (*position);
                        *position /= position.w;
                    }

                    frame.position_buffer.write_slice(offset, &positions);
                    frame.color_buffer.write_slice(offset, &[color; 24]);
                    offset += 24;
                }
                DebugObject::RectPrism {
                    half_extents,
                    transform,
                } => {
                    let half_extents =
                        Vec4::new(half_extents.x, half_extents.y, half_extents.z, 1.0);

                    let mut positions = [
                        // Vertical lines
                        half_extents * Vec4::new(-1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, 1.0, 1.0),
                        half_extents * Vec4::new(1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, 1.0, 1.0),
                        // Depth lines
                        half_extents * Vec4::new(-1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, 1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, 1.0, 1.0),
                        // Horizontal lines
                        half_extents * Vec4::new(1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, -1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, -1.0, 1.0),
                        half_extents * Vec4::new(1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(-1.0, -1.0, 1.0, 1.0),
                        half_extents * Vec4::new(1.0, 1.0, 1.0, 1.0),
                        half_extents * Vec4::new(-1.0, 1.0, 1.0, 1.0),
                    ];

                    for position in &mut positions {
                        *position = transform * (*position);
                    }

                    frame.position_buffer.write_slice(offset, &positions);
                    frame.color_buffer.write_slice(offset, &[color; 24]);
                    offset += 24;
                }
            }
        }

        // Bind camera set
        let sets = [camera.set];
        let offsets = [
            camera.ubo.aligned_size() as u32 * frame_idx as u32,
            camera.aligned_cluster_size as u32 * frame_idx as u32,
        ];
        device.cmd_bind_descriptor_sets(
            commands,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &sets,
            &offsets,
        );

        // Bind pipeline
        device.cmd_bind_pipeline(commands, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

        // Bind vertex buffers
        let buffers = [frame.position_buffer.buffer(), frame.color_buffer.buffer()];
        let offsets = [0, 0];

        device.cmd_bind_vertex_buffers(commands, 0, &buffers, &offsets);

        // Draw everything
        device.cmd_draw(commands, point_count as u32, 1, 0, 0);
    }
}

impl Drop for DebugDrawingInner {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.ctx.0.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl FrameData {
    unsafe fn new(ctx: &GraphicsContext) -> Self {
        let position_buffer = BufferArray::new(
            ctx,
            DEFAULT_DEBUG_BUFFER_CAP,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        );
        let color_buffer = BufferArray::new(
            ctx,
            DEFAULT_DEBUG_BUFFER_CAP,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        );

        Self {
            position_buffer,
            color_buffer,
        }
    }
}

impl DebugDrawingApi<VkBackend> for DebugDrawing {
    fn draw_line(&self, a: Vec3, b: Vec3, color: Vec3) {
        let mut draws = self.0.draws.lock().expect("mutex poisoned");
        draws.push((DebugObject::Line { a, b }, color));
    }

    fn draw_sphere(&self, _: Vec3, _: f32, _: Vec3) {
        todo!();
        // let mut draws = self.0.draws.lock().expect("mutex poisoned");
        // draws.push((DebugObject::Sphere { center, radius }, color));
    }

    fn draw_frustum(&self, descriptor: CameraDescriptor, color: Vec3) {
        let mut draws = self.0.draws.lock().expect("mutex poisoned");
        draws.push((DebugObject::Frustum(descriptor), color));
    }

    fn draw_rect_prism(&self, half_extents: Vec3, transform: Mat4, color: Vec3) {
        let mut draws = self.0.draws.lock().expect("mutex poisoned");
        draws.push((
            DebugObject::RectPrism {
                half_extents,
                transform,
            },
            color,
        ));
    }
}

unsafe impl Send for DebugDrawing {}
unsafe impl Sync for DebugDrawing {}

const DEBUG_DRAW_VERT_CODE: &[u8] = include_bytes!("debug_draw.vert.spv");

const DEBUG_DRAW_FRAG_CODE: &[u8] = include_bytes!("debug_draw.frag.spv");
