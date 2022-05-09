use ash::vk;
use bytemuck::{Pod, Zeroable};
use factory::descriptors::DescriptorPool;
use glam::IVec2;
use gpu_alloc::UsageFlags;

use crate::alloc::*;
use crate::prelude::*;

const ZREDUCE_SETS_PER_POOL: usize = 8;

const DEPTH_PYRAMID_FORMAT: vk::Format = vk::Format::R32_SFLOAT;

pub(crate) struct DepthPyramidGenerator {
    ctx: GraphicsContext,
    /// Allocates sets used for mip generation.
    pool: DescriptorPool,
    /// Sampler for generating depth pyramid mips.
    sampler: vk::Sampler,
    reduce_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}

/// Depth pyramid used for hierarchical-z buffer culling.
////
/// ## Note
/// The depth pyramid does not contain LOD0 of the depth image that it is downsampling. This
/// is done for performance reasons.
pub(crate) struct DepthPyramid {
    /// Dimensions of the target depth image to downsample.
    dimensions: (u32, u32),
    /// Output depth pyramid image.
    image: Image,
    /// View for the entire depth pyramid, including all mips.
    view: vk::ImageView,
    /// Individual view for each mip in the image.
    mip_views: Vec<vk::ImageView>,
    /// Framebuffer to render to when reducing for each frame.
    framebuffers: Vec<vk::Framebuffer>,
    /// Descriptor sets used for generating the mip levels.
    sets: Vec<vk::DescriptorSet>,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct ZReduceInfo {
    image_size: IVec2,
}

unsafe impl Pod for ZReduceInfo {}
unsafe impl Zeroable for ZReduceInfo {}

impl DepthPyramidGenerator {
    pub unsafe fn new(ctx: &GraphicsContext) -> Self {
        let pool = {
            let bindings = [
                // Input z-buffer texture to downscale.
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ];

            let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &create_info, ZREDUCE_SETS_PER_POOL)
        };

        let layout = {
            let layouts = [pool.layout()];

            let push_ranges = [vk::PushConstantRange::builder()
                .offset(0)
                .size(std::mem::size_of::<ZReduceInfo>() as u32)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build()];

            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&push_ranges)
                .build();

            ctx.0
                .device
                .create_pipeline_layout(&create_info, None)
                .expect("Unable to create z-culling pipeline layout")
        };

        let reduce_pass = {
            let attachments = [vk::AttachmentDescription::builder()
                .format(DEPTH_PYRAMID_FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .build()];

            let attachment_refs = [vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }];

            let dependencies = [vk::SubpassDependency::builder()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .dependency_flags(vk::DependencyFlags::BY_REGION)
                .build()];

            let subpasses = [vk::SubpassDescription::builder()
                .color_attachments(&attachment_refs)
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .build()];

            let render_pass_create_info = vk::RenderPassCreateInfo::builder()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies)
                .build();

            ctx.0
                .device
                .create_render_pass(&render_pass_create_info, None)
                .expect("unable to create z-reduction render pass")
        };

        let vert_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: ZREDUCE_VERT_SHADER_CODE.as_ptr() as *const u32,
                code_size: ZREDUCE_VERT_SHADER_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create quad vertex shader module")
        };

        let frag_module = {
            let create_info = vk::ShaderModuleCreateInfo {
                p_code: ZREDUCE_FRAG_SHADER_CODE.as_ptr() as *const u32,
                code_size: ZREDUCE_FRAG_SHADER_CODE.len(),
                ..Default::default()
            };

            ctx.0
                .device
                .create_shader_module(&create_info, None)
                .expect("Unable to create depth reducing fragment shader module")
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
                .color_write_mask(vk::ColorComponentFlags::R)
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
                .layout(layout)
                .render_pass(reduce_pass)
                .subpass(0)
                .build()];

            ctx.0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
                .expect("unable to create z-reduction pipeline")[0]
        };

        ctx.0.device.destroy_shader_module(vert_module, None);
        ctx.0.device.destroy_shader_module(frag_module, None);

        let sampler = {
            // TODO: It would be more portable to simulate this sampler mode
            let mut max_filter = vk::SamplerReductionModeCreateInfo::builder()
                .reduction_mode(vk::SamplerReductionMode::MAX)
                .build();

            let create_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .min_lod(0.0)
                .max_lod(vk::LOD_CLAMP_NONE)
                .anisotropy_enable(false)
                .push_next(&mut max_filter)
                .build();

            ctx.0
                .device
                .create_sampler(&create_info, None)
                .expect("Unable to create z-buffer culling sampler")
        };

        Self {
            ctx: ctx.clone(),
            pool,
            layout,
            pipeline,
            sampler,
            reduce_pass,
        }
    }

    pub fn sampler(&self) -> vk::Sampler {
        self.sampler
    }

    pub unsafe fn allocate(&mut self, width: u32, height: u32) -> DepthPyramid {
        DepthPyramid::new(
            &self.ctx,
            &mut self.pool,
            self.sampler,
            self.reduce_pass,
            width,
            height,
        )
    }

    pub unsafe fn free(&mut self, pyramid: DepthPyramid) {
        pyramid.release(&mut self.pool);
    }

    /// Generate depth pyramid given a source depth image.
    pub unsafe fn gen_pyramid(
        &self,
        commands: vk::CommandBuffer,
        src: &Image,
        src_view: vk::ImageView,
        dst: &DepthPyramid,
    ) {
        assert_eq!(src.width(), dst.dimensions.0);
        assert_eq!(src.height(), dst.dimensions.1);

        // Image usage is currently transfer source, so we have to make it shader read only
        let barrier = [vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(src.image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
            )
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .build()];

        self.ctx.0.device.cmd_pipeline_barrier(
            commands,
            vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::BY_REGION,
            &[],
            &[],
            &barrier,
        );

        // Update set with source image
        let src_img = [vk::DescriptorImageInfo::builder()
            .image_view(src_view)
            .sampler(self.sampler)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build()];

        let writes = [vk::WriteDescriptorSet::builder()
            .dst_array_element(0)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .dst_set(dst.sets[0])
            .image_info(&src_img)
            .build()];

        self.ctx.0.device.update_descriptor_sets(&writes, &[]);

        for (i, set) in dst.sets.iter().enumerate() {
            let level_width = (dst.image.width() >> i).max(1);
            let level_height = (dst.image.height() >> i).max(1);

            let viewport = [vk::Viewport {
                width: level_width as f32,
                height: level_height as f32,
                x: 0.0,
                y: 0.0,
                min_depth: 0.0,
                max_depth: 1.0,
            }];

            let scissor = [vk::Rect2D {
                extent: vk::Extent2D {
                    width: level_width,
                    height: level_height,
                },
                offset: vk::Offset2D { x: 0, y: 0 },
            }];

            let rp_begin_info = vk::RenderPassBeginInfo::builder()
                .render_pass(self.reduce_pass)
                .framebuffer(dst.framebuffers[i])
                .render_area(scissor[0])
                .build();

            self.ctx.0.device.cmd_set_viewport(commands, 0, &viewport);

            self.ctx.0.device.cmd_set_scissor(commands, 0, &scissor);

            self.ctx.0.device.cmd_begin_render_pass(
                commands,
                &rp_begin_info,
                vk::SubpassContents::INLINE,
            );

            // Perform z-buffer reduction
            self.ctx.0.device.cmd_bind_pipeline(
                commands,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );

            let sets = [*set];
            self.ctx.0.device.cmd_bind_descriptor_sets(
                commands,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &sets,
                &[],
            );

            let zreduce_info = [ZReduceInfo {
                image_size: IVec2::new(
                    (src.width() >> i).max(1) as i32,
                    (src.height() >> i).max(1) as i32,
                ),
            }];

            self.ctx.0.device.cmd_push_constants(
                commands,
                self.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                bytemuck::cast_slice(&zreduce_info),
            );

            self.ctx.0.device.cmd_draw(commands, 3, 1, 0, 0);

            self.ctx.0.device.cmd_end_render_pass(commands);
        }

        let barrier = [vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(src.image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .build()];

        self.ctx.0.device.cmd_pipeline_barrier(
            commands,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::DependencyFlags::BY_REGION,
            &[],
            &[],
            &barrier,
        );
    }
}

impl Drop for DepthPyramidGenerator {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .device
                .destroy_render_pass(self.reduce_pass, None);
            self.ctx.0.device.destroy_sampler(self.sampler, None);
            self.ctx.0.device.destroy_pipeline(self.pipeline, None);
            self.ctx.0.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

impl DepthPyramid {
    unsafe fn new(
        ctx: &GraphicsContext,
        pool: &mut DescriptorPool,
        sampler: vk::Sampler,
        pass: vk::RenderPass,
        width: u32,
        height: u32,
    ) -> Self {
        let dimensions = (width, height);
        let mip_levels = (width.max(height) as f32).log2().floor() as u32;
        let width = width / 2;
        let height = height / 2;

        let create_info = ImageCreateInfo {
            ctx: ctx.clone(),
            width,
            height,
            memory_usage: UsageFlags::FAST_DEVICE_ACCESS,
            image_usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            mip_levels,
            array_layers: 1,
            format: DEPTH_PYRAMID_FORMAT,
        };

        let image = Image::new(&create_info);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image.image())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(DEPTH_PYRAMID_FORMAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        let view = ctx
            .0
            .device
            .create_image_view(&create_info, None)
            .expect("unable to create depth pyramid image view");

        let mut mip_views = Vec::with_capacity(mip_levels as usize);
        let mut framebuffers = Vec::with_capacity(mip_levels as usize);
        for i in 0..mip_levels {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(image.image())
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(DEPTH_PYRAMID_FORMAT)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: i,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            let mip_view = ctx
                .0
                .device
                .create_image_view(&create_info, None)
                .expect("unable to create depth pyramid image view");

            let attachments = [mip_view];

            let create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(pass)
                .attachments(&attachments)
                .width((width >> i).max(1))
                .height((height >> i).max(1))
                .layers(1)
                .build();

            framebuffers.push(
                ctx.0
                    .device
                    .create_framebuffer(&create_info, None)
                    .expect("unable to create depth reduction framebuffer"),
            );

            mip_views.push(mip_view);
        }

        let mut sets = Vec::with_capacity(mip_levels as usize);
        for _ in 0..mip_levels {
            sets.push(pool.allocate());
        }

        // Update sets with mip views
        for (i, set) in sets[1..].iter().enumerate() {
            let src_img = [vk::DescriptorImageInfo::builder()
                .image_view(mip_views[i])
                .sampler(sampler)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .build()];

            let writes = [vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_set(*set)
                .image_info(&src_img)
                .build()];

            ctx.0.device.update_descriptor_sets(&writes, &[]);
        }

        Self {
            dimensions,
            image,
            view,
            mip_views,
            framebuffers,
            sets,
        }
    }

    #[inline]
    pub fn target_dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    #[inline]
    pub fn view(&self) -> vk::ImageView {
        self.view
    }

    unsafe fn release(mut self, pool: &mut DescriptorPool) {
        for view in self.mip_views.drain(..) {
            self.image.ctx.0.device.destroy_image_view(view, None);
        }

        for framebuffer in self.framebuffers.drain(..) {
            self.image
                .ctx
                .0
                .device
                .destroy_framebuffer(framebuffer, None);
        }

        for set in self.sets.drain(..) {
            pool.free(set);
        }

        self.image.ctx.0.device.destroy_image_view(self.view, None);
    }
}

const ZREDUCE_FRAG_SHADER_CODE: &[u8] = include_bytes!("zreduce.frag.spv");
const ZREDUCE_VERT_SHADER_CODE: &[u8] = include_bytes!("quad.vert.spv");
