use std::{collections::HashSet, hash::Hash};

use ard_render_graph::{
    buffer::{BufferAccessDescriptor, BufferDescriptor, BufferUsage},
    context::Context,
    graph::{self, ImageState, ImageUsage, RenderGraphBuildState, RenderGraphResources},
    image::{ImageAccessDecriptor, ImageDescriptor, ImageId, SizeGroup, SizeGroupId},
    pass::{ClearColor, Pass, PassDescriptor, PassFn},
    LoadOp, Operations,
};
use ash::vk;

use crate::{
    alloc::{
        Image, ImageCreateInfo, ReadBackStorageBuffer, StorageBuffer, UniformBuffer,
        WriteStorageBuffer,
    },
    context::GraphicsContext,
    shader_constants::FRAMES_IN_FLIGHT,
};

pub(crate) struct RenderGraphContext<T> {
    ctx: GraphicsContext,
    /// Index of the current frame in flight.
    frame: usize,
    /// Command pool for rendering commands.
    command_pool: vk::CommandPool,
    /// Command buffers for each frame in flight.
    command_buffers: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
    _phantom: std::marker::PhantomData<T>,
}

pub(crate) enum RenderPass<T> {
    Graphics {
        enabled: bool,
        ctx: GraphicsContext,
        /// Code to run during the pass.
        code: PassFn<RenderGraphContext<T>>,
        /// Vulkan render pass to begin.
        pass: vk::RenderPass,
        /// Memory barriers for buffers used during rendering.
        barriers: Vec<Barrier>,
        /// Clear values for the frame buffer.
        clear_values: Vec<vk::ClearValue>,
        /// Frame buffer containing all images in the render pass.
        frame_buffers: [vk::Framebuffer; FRAMES_IN_FLIGHT],
        /// Contains the image views used by each frame buffer. When the pass is run, if the
        /// current image views for the images bound don't match, then we must recreate the frame
        /// buffer.
        frame_buffer_views: [HashSet<vk::ImageView>; FRAMES_IN_FLIGHT],
        /// Size group ID for images bound to frame buffers so we can update them during a resize.
        size_group: SizeGroupId,
        /// ID of depth attachment.
        depth_attachment: Option<ImageId>,
        /// ID of color attachments.
        color_attachments: Vec<ImageId>,
    },
    Compute {
        enabled: bool,
        code: PassFn<RenderGraphContext<T>>,
        /// Memory barriers for buffers used during rendering.
        buffer_barriers: Vec<Barrier>,
    },
    Cpu {
        enabled: bool,
        code: PassFn<RenderGraphContext<T>>,
    },
}

pub(crate) enum GraphBuffer {
    Uniform {
        buffers: Vec<UniformBuffer>,
    },
    Storage {
        buffers: Vec<StorageBuffer>,
    },
    /// TODO
    ReadStorage,
    WriteStorage {
        buffers: Vec<WriteStorageBuffer>,
    },
    ReadBack {
        buffers: Vec<ReadBackStorageBuffer>,
    },
}

pub(crate) struct RenderTarget {
    pub image: Image,
    pub view: vk::ImageView,
}

#[derive(Copy, Clone)]
pub(crate) struct Barrier {
    /// NOTE: Held in an array for easy Vulkan usage.
    memory_barriers: [vk::MemoryBarrier; 1],
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
}

impl Hash for Barrier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.memory_barriers[0].dst_access_mask.hash(state);
        self.memory_barriers[0].src_access_mask.hash(state);
        self.src_stage.hash(state);
        self.dst_stage.hash(state);
    }
}

impl PartialEq for Barrier {
    fn eq(&self, other: &Self) -> bool {
        self.memory_barriers[0].src_access_mask == other.memory_barriers[0].src_access_mask
            && self.memory_barriers[0].dst_access_mask == other.memory_barriers[0].dst_access_mask
            && self.src_stage == other.src_stage
            && self.dst_stage == other.dst_stage
    }
}

impl Eq for Barrier {}

unsafe impl Send for Barrier {}
unsafe impl Sync for Barrier {}

impl<T> Pass<RenderGraphContext<T>> for RenderPass<T> {
    fn run(
        &mut self,
        command_buffer: &vk::CommandBuffer,
        rg_ctx: &mut RenderGraphContext<T>,
        state: &mut T,
        resources: &mut RenderGraphResources<RenderGraphContext<T>>,
    ) {
        let device = rg_ctx.ctx.0.device.clone();

        // Check for frame buffer validity for graphics passes
        let mut has_valid_views = true;
        if let RenderPass::Graphics {
            depth_attachment,
            color_attachments,
            frame_buffer_views,
            ..
        } = self
        {
            if let Some(attachment) = depth_attachment {
                let view = resources.get_image(*attachment).unwrap().1[rg_ctx.frame].view;
                has_valid_views = frame_buffer_views[rg_ctx.frame].contains(&view);
            }

            if has_valid_views {
                for attachment in color_attachments {
                    let view = resources.get_image(*attachment).unwrap().1[rg_ctx.frame].view;
                    if !frame_buffer_views[rg_ctx.frame].contains(&view) {
                        has_valid_views = false;
                        break;
                    }
                }
            }
        }

        if !has_valid_views {
            unsafe {
                self.destroy_framebuffers();
                self.create_framebuffers(resources);
            }
        }

        match self {
            RenderPass::Graphics {
                enabled,
                code,
                pass,
                clear_values,
                frame_buffers,
                size_group,
                barriers,
                ..
            } => {
                let size_group = resources.get_size_group(*size_group);

                // Signal buffer barriers
                for barrier in barriers {
                    unsafe {
                        device.cmd_pipeline_barrier(
                            *command_buffer,
                            barrier.src_stage,
                            barrier.dst_stage,
                            vk::DependencyFlags::BY_REGION,
                            &barrier.memory_barriers,
                            &[],
                            &[],
                        );
                    }
                }

                if !*enabled {
                    return;
                }

                // NOTE: Viewport is flipped to account for Vulkan coordinate system
                let viewport = [vk::Viewport {
                    width: size_group.width as f32,
                    height: -(size_group.height as f32),
                    x: 0.0,
                    y: size_group.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }];

                let scissor = [vk::Rect2D {
                    extent: vk::Extent2D {
                        width: size_group.width,
                        height: size_group.height,
                    },
                    offset: vk::Offset2D { x: 0, y: 0 },
                }];

                let rp_begin_info = vk::RenderPassBeginInfo::builder()
                    .clear_values(clear_values)
                    .render_pass(*pass)
                    .framebuffer(frame_buffers[rg_ctx.frame])
                    .render_area(scissor[0])
                    .build();

                unsafe {
                    device.cmd_set_viewport(*command_buffer, 0, &viewport);
                    device.cmd_set_scissor(*command_buffer, 0, &scissor);
                    device.cmd_begin_render_pass(
                        *command_buffer,
                        &rp_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                }

                code(rg_ctx, state, command_buffer, self, resources);

                unsafe {
                    device.cmd_end_render_pass(*command_buffer);
                }
            }
            RenderPass::Compute {
                enabled,
                code,
                buffer_barriers,
            } => {
                // Signal buffer barriers
                for barrier in buffer_barriers {
                    unsafe {
                        device.cmd_pipeline_barrier(
                            *command_buffer,
                            barrier.src_stage,
                            barrier.dst_stage,
                            vk::DependencyFlags::BY_REGION,
                            &barrier.memory_barriers,
                            &[],
                            &[],
                        );
                    }
                }

                if !*enabled {
                    return;
                }

                code(rg_ctx, state, command_buffer, self, resources);
            }
            RenderPass::Cpu { enabled, code } => {
                if !*enabled {
                    return;
                }

                code(rg_ctx, state, command_buffer, self, resources);
            }
        }
    }

    fn toggle(&mut self, is_enabled: bool) {
        match self {
            RenderPass::Graphics { enabled, .. } => *enabled = is_enabled,
            RenderPass::Compute { enabled, .. } => *enabled = is_enabled,
            RenderPass::Cpu { enabled, .. } => *enabled = is_enabled,
        }
    }
}

impl<T> Context for RenderGraphContext<T> {
    type State = T;
    type Buffer = GraphBuffer;
    type Image = (SizeGroupId, Vec<RenderTarget>);
    type ImageFormat = vk::Format;
    type CommandBuffer = vk::CommandBuffer;
    type Pass = RenderPass<T>;

    fn create_buffer(
        &mut self,
        descriptor: &BufferDescriptor,
        _resources: &RenderGraphResources<Self>,
    ) -> Self::Buffer {
        match descriptor.usage {
            BufferUsage::UniformBuffer => todo!(),
            BufferUsage::StorageBuffer => {
                let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
                for _ in 0..FRAMES_IN_FLIGHT {
                    buffers
                        .push(unsafe { StorageBuffer::new(&self.ctx, descriptor.size as usize) });
                }

                GraphBuffer::Storage { buffers }
            }
            BufferUsage::ReadStorageBuffer => todo!(),
            BufferUsage::WriteStorageBuffer => {
                let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
                for _ in 0..FRAMES_IN_FLIGHT {
                    buffers.push(unsafe {
                        WriteStorageBuffer::new(&self.ctx, descriptor.size as usize)
                    });
                }

                GraphBuffer::WriteStorage { buffers }
            }
            BufferUsage::ReadBack => {
                let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
                for _ in 0..FRAMES_IN_FLIGHT {
                    buffers.push(unsafe {
                        ReadBackStorageBuffer::new(&self.ctx, descriptor.size as usize)
                    });
                }

                GraphBuffer::ReadBack { buffers }
            }
        }
    }

    fn create_image(
        &mut self,
        descriptor: &ImageDescriptor<Self>,
        resources: &RenderGraphResources<Self>,
    ) -> Self::Image {
        let size_group = resources.get_size_group(descriptor.size_group);

        let mut images = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            images.push(unsafe { RenderTarget::new(&self.ctx, size_group, descriptor) });
        }

        (descriptor.size_group, images)
    }

    fn create_pass(
        &mut self,
        descriptor: &PassDescriptor<Self>,
        state: &RenderGraphBuildState,
        resources: &RenderGraphResources<Self>,
    ) -> Self::Pass {
        match descriptor {
            PassDescriptor::RenderPass {
                color_attachments,
                depth_stencil_attachment,
                buffers,
                images,
                code,
                ..
            } => {
                let total_attachments = if depth_stencil_attachment.is_some() {
                    1 + color_attachments.len()
                } else {
                    color_attachments.len()
                };

                let mut all_size_group = None;
                let mut attachments = Vec::with_capacity(total_attachments);
                let mut attachment_refs = Vec::with_capacity(total_attachments);
                let mut dependencies = Vec::with_capacity(total_attachments);
                let mut clear_values = Vec::with_capacity(total_attachments);
                let barriers =
                    create_barriers(buffers, images, state, depth_stencil_attachment.is_some());

                // Create depth attachment
                if let Some(attachment) = depth_stencil_attachment {
                    let (size_group, images) = resources.get_image(attachment.image).unwrap();
                    let image_state = state.get_image_state(attachment.image).unwrap();

                    all_size_group = Some(*size_group);

                    let (descriptor, clear_value, dependency) = create_attachment_descriptor(
                        &attachment.ops,
                        image_state,
                        &images[0].image,
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    );

                    if let Some((depth, stencil)) = clear_value {
                        clear_values.push(vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue { depth, stencil },
                        });
                    } else {
                        // Unused by Vulkan
                        clear_values.push(vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue {
                                depth: 0.0,
                                stencil: 0,
                            },
                        });
                    }

                    if let Some(dependency) = dependency {
                        dependencies.push(dependency);
                    }

                    attachments.push(descriptor);

                    attachment_refs.push(vk::AttachmentReference {
                        attachment: 0,
                        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                    });
                }

                // Create color attachment(s)
                for (i, attachment) in color_attachments.iter().enumerate() {
                    let (size_group, images) = resources.get_image(attachment.image).unwrap();
                    let image_state = state.get_image_state(attachment.image).unwrap();

                    all_size_group = Some(*size_group);

                    let (descriptor, clear_value, dependency) = create_attachment_descriptor(
                        &attachment.ops,
                        image_state,
                        &images[0].image,
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    );

                    if let Some(color) = clear_value {
                        clear_values.push(vk::ClearValue {
                            color: match color {
                                ClearColor::RGBAF32(val) => vk::ClearColorValue { float32: val },
                                ClearColor::RGBAU32(val) => vk::ClearColorValue { uint32: val },
                            },
                        });
                    } else {
                        // Unused by Vulkan
                        clear_values.push(vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: [0.0, 0.0, 0.0, 0.0],
                            },
                        });
                    }

                    if let Some(dependency) = dependency {
                        dependencies.push(dependency);
                    }

                    attachments.push(descriptor);

                    attachment_refs.push(vk::AttachmentReference {
                        attachment: i as u32
                            + if depth_stencil_attachment.is_some() {
                                1
                            } else {
                                0
                            },
                        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    });
                }

                // Create render pass
                let mut subpass_builder = vk::SubpassDescription::builder()
                    .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

                if depth_stencil_attachment.is_some() {
                    subpass_builder = subpass_builder
                        .depth_stencil_attachment(&attachment_refs[0])
                        .color_attachments(&attachment_refs[1..(1 + color_attachments.len())]);
                } else {
                    subpass_builder = subpass_builder
                        .color_attachments(&attachment_refs[0..color_attachments.len()]);
                }

                let subpass = [subpass_builder.build()];

                let render_pass_create_info = vk::RenderPassCreateInfo::builder()
                    .attachments(&attachments)
                    .subpasses(&subpass)
                    .dependencies(&dependencies)
                    .build();

                let render_pass = unsafe {
                    self.ctx
                        .0
                        .device
                        .create_render_pass(&render_pass_create_info, None)
                        .expect("unable to create render pass in render graph")
                };

                let mut rp_color_attachments = Vec::with_capacity(color_attachments.len());
                for attachment in color_attachments {
                    rp_color_attachments.push(attachment.image);
                }

                let mut render_pass = RenderPass::Graphics {
                    enabled: true,
                    ctx: self.ctx.clone(),
                    code: *code,
                    pass: render_pass,
                    clear_values,
                    frame_buffers: [vk::Framebuffer::null(); FRAMES_IN_FLIGHT],
                    frame_buffer_views: Default::default(),
                    size_group: all_size_group
                        .expect("graphics pass guaranteed to have at least one image"),
                    depth_attachment: depth_stencil_attachment
                        .as_ref()
                        .map(|attachment| attachment.image),
                    color_attachments: rp_color_attachments,
                    barriers,
                };

                unsafe {
                    render_pass.create_framebuffers(resources);
                }

                render_pass
            }
            PassDescriptor::ComputePass {
                buffers,
                images,
                code,
                ..
            } => {
                let buffer_barriers = create_barriers(buffers, images, state, false);
                RenderPass::Compute {
                    enabled: true,
                    code: *code,
                    buffer_barriers,
                }
            }
            PassDescriptor::CPUPass { code, .. } => RenderPass::Cpu {
                enabled: true,
                code: *code,
            },
        }
    }
}

impl<T> RenderGraphContext<T> {
    pub unsafe fn new(ctx: &GraphicsContext) -> Self {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(ctx.0.queue_family_indices.main)
            .build();

        let command_pool = ctx
            .0
            .device
            .create_command_pool(&create_info, None)
            .expect("unable to create command pool in render graph context");

        let create_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(FRAMES_IN_FLIGHT as u32)
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .build();

        let command_buffers_vec = ctx
            .0
            .device
            .allocate_command_buffers(&create_info)
            .expect("unable to allocate command buffers in render graph context");
        let mut command_buffers = [vk::CommandBuffer::null(); FRAMES_IN_FLIGHT];
        for (i, command_buffer) in command_buffers_vec.into_iter().enumerate() {
            command_buffers[i] = command_buffer;
        }

        Self {
            ctx: ctx.clone(),
            frame: 0,
            command_pool,
            command_buffers,
            _phantom: std::marker::PhantomData::default(),
        }
    }

    #[inline]
    pub fn frame(&self) -> usize {
        self.frame
    }

    #[inline]
    pub fn next_frame(&mut self) -> usize {
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        self.frame
    }

    #[inline]
    pub fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffers[self.frame]
    }
}

impl<T> Drop for RenderGraphContext<T> {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .0
                .device
                .destroy_command_pool(self.command_pool, None);
        }
    }
}

impl<T> RenderPass<T> {
    /// No-op if not a graphics pass.
    unsafe fn create_framebuffers(
        &mut self,
        resources: &RenderGraphResources<RenderGraphContext<T>>,
    ) {
        if let RenderPass::Graphics {
            ctx,
            frame_buffers,
            frame_buffer_views,
            depth_attachment,
            color_attachments,
            pass,
            size_group,
            ..
        } = self
        {
            let size_group = resources.get_size_group(*size_group);

            let attachment_count = if depth_attachment.is_some() {
                1 + color_attachments.len()
            } else {
                color_attachments.len()
            };

            let mut attachments = vec![vk::ImageView::null(); attachment_count];

            for frame in 0..FRAMES_IN_FLIGHT {
                let mut idx = 0;

                frame_buffer_views[frame].clear();

                if let Some(attachment) = depth_attachment {
                    let images = &resources
                        .get_image(*attachment)
                        .expect("depth stencil attachment used in pass but does not exist")
                        .1;
                    let view = images[frame].view;
                    attachments[idx] = view;
                    frame_buffer_views[frame].insert(view);
                    idx += 1;
                }

                for attachment in color_attachments.iter() {
                    let images = &resources
                        .get_image(*attachment)
                        .expect("color attachment used in pass but does not exist")
                        .1;
                    let view = images[frame].view;
                    attachments[idx] = view;
                    frame_buffer_views[frame].insert(view);
                    idx += 1;
                }

                let create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(*pass)
                    .attachments(&attachments)
                    .width(size_group.width)
                    .height(size_group.height)
                    .layers(1)
                    .build();

                frame_buffers[frame] = ctx
                    .0
                    .device
                    .create_framebuffer(&create_info, None)
                    .expect("unable to create depth only framebuffer");
            }
        }
    }

    /// No-op if not a graphics pass.
    unsafe fn destroy_framebuffers(&mut self) {
        if let RenderPass::Graphics {
            ctx, frame_buffers, ..
        } = self
        {
            for framebuffer in frame_buffers {
                ctx.0.device.destroy_framebuffer(*framebuffer, None);
            }
        }
    }
}

impl<T> Drop for RenderPass<T> {
    fn drop(&mut self) {
        match self {
            RenderPass::Graphics {
                ctx,
                pass,
                frame_buffers,
                ..
            } => unsafe {
                ctx.0.device.destroy_render_pass(*pass, None);
                for framebuffer in frame_buffers {
                    ctx.0.device.destroy_framebuffer(*framebuffer, None);
                }
            },
            RenderPass::Compute { .. } => {}
            RenderPass::Cpu { .. } => {}
        }
    }
}

impl RenderTarget {
    unsafe fn new<T>(
        ctx: &GraphicsContext,
        size_group: &SizeGroup,
        descriptor: &ImageDescriptor<RenderGraphContext<T>>,
    ) -> Self {
        let create_info = ImageCreateInfo {
            ctx: ctx.clone(),
            ty: vk::ImageType::TYPE_2D,
            width: size_group.width,
            height: size_group.height,
            depth: 1,
            memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
            image_usage: match descriptor.format {
                vk::Format::D24_UNORM_S8_UINT | vk::Format::D32_SFLOAT_S8_UINT => {
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                }
                _ => vk::ImageUsageFlags::COLOR_ATTACHMENT,
            } | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::SAMPLED,
            mip_levels: size_group.mip_levels,
            array_layers: size_group.array_layers,
            format: descriptor.format,
            flags: vk::ImageCreateFlags::empty(),
        };

        let image = Image::new(&create_info);

        let create_info = vk::ImageViewCreateInfo {
            image: image.image(),
            view_type: vk::ImageViewType::TYPE_2D,
            format: descriptor.format,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: match descriptor.format {
                    vk::Format::D24_UNORM_S8_UINT | vk::Format::D32_SFLOAT_S8_UINT => {
                        vk::ImageAspectFlags::DEPTH
                    }
                    _ => vk::ImageAspectFlags::COLOR,
                },
                base_mip_level: 0,
                level_count: size_group.mip_levels,
                base_array_layer: 0,
                layer_count: size_group.array_layers,
            },
            ..Default::default()
        };

        let view = ctx
            .0
            .device
            .create_image_view(&create_info, None)
            .expect("unable to create image view in render graph");

        Self { image, view }
    }
}

impl Drop for RenderTarget {
    fn drop(&mut self) {
        unsafe {
            self.image.ctx.0.device.destroy_image_view(self.view, None);
        }
    }
}

impl GraphBuffer {
    #[inline]
    pub fn expect_storage(&self, frame: usize) -> &StorageBuffer {
        if let GraphBuffer::Storage { buffers } = self {
            &buffers[frame]
        } else {
            panic!("expected storage buffer");
        }
    }

    #[inline]
    pub fn expect_storage_mut(&mut self, frame: usize) -> &mut StorageBuffer {
        if let GraphBuffer::Storage { buffers } = self {
            &mut buffers[frame]
        } else {
            panic!("expected storage buffer");
        }
    }

    #[inline]
    pub fn expect_write_storage(&self, frame: usize) -> &WriteStorageBuffer {
        if let GraphBuffer::WriteStorage { buffers } = self {
            &buffers[frame]
        } else {
            panic!("expected write storage buffer");
        }
    }

    #[inline]
    pub fn expect_write_storage_mut(&mut self, frame: usize) -> &mut WriteStorageBuffer {
        if let GraphBuffer::WriteStorage { buffers } = self {
            &mut buffers[frame]
        } else {
            panic!("expected write storage buffer");
        }
    }

    #[inline]
    pub fn expect_read_back(&self, frame: usize) -> &ReadBackStorageBuffer {
        if let GraphBuffer::ReadBack { buffers } = self {
            &buffers[frame]
        } else {
            panic!("expected read back storage buffer");
        }
    }

    #[inline]
    pub fn expect_read_back_mut(&mut self, frame: usize) -> &mut ReadBackStorageBuffer {
        if let GraphBuffer::ReadBack { buffers } = self {
            &mut buffers[frame]
        } else {
            panic!("expected read back storage buffer");
        }
    }
}

/// Helper to create buffer barriers.
fn create_barriers(
    buffers: &[BufferAccessDescriptor],
    images: &[ImageAccessDecriptor],
    state: &RenderGraphBuildState,
    has_depth: bool,
) -> Vec<Barrier> {
    let mut barriers = HashSet::<Barrier>::default();

    // Create buffer barriers
    for buffer in buffers {
        let buffer_state = state.get_buffer_state(buffer.buffer).unwrap();

        // No barrier needed if not yet used
        if buffer_state.last.0 == graph::BufferUsage::Unused {
            continue;
        }

        let (src_access, src_stage) = match buffer_state.last.0 {
            graph::BufferUsage::Unused => unreachable!(),
            graph::BufferUsage::Graphics => (
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ),
            graph::BufferUsage::Compute => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            ),
        };

        let (dst_access, dst_stage) = match buffer_state.current.0 {
            graph::BufferUsage::Unused => unreachable!(),
            graph::BufferUsage::Graphics =>
            // Compute shader can do anything, so we must wait at the top
            {
                if buffer_state.last.0 == graph::BufferUsage::Compute {
                    (
                        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                    )
                }
                // Depth stencil is used so we wait for depth testing
                else if has_depth {
                    (
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    )
                }
                // Only color output for wait
                else {
                    (
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    )
                }
            }
            graph::BufferUsage::Compute => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            ),
        };

        barriers.insert(Barrier {
            memory_barriers: [vk::MemoryBarrier::builder()
                .src_access_mask(src_access)
                .dst_access_mask(dst_access)
                .build()],
            src_stage,
            dst_stage,
        });
    }

    // Create image barriers
    for image in images {
        let image_state = state.get_image_state(image.image).unwrap();

        // No barrier needed if not yet used
        if image_state.last.0 == graph::ImageUsage::Unused {
            continue;
        }

        let (src_access, src_stage) = match image_state.last.0 {
            graph::ImageUsage::Unused => unreachable!(),
            graph::ImageUsage::ColorAttachment => (
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ),
            graph::ImageUsage::Compute => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            ),
            ImageUsage::DepthStencilAttachment => (
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            ),
            ImageUsage::Sampled => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            ),
        };

        let (dst_access, dst_stage) = match image_state.current.0 {
            graph::ImageUsage::Unused => unreachable!(),
            graph::ImageUsage::ColorAttachment => (
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ),
            graph::ImageUsage::Compute => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            ),
            ImageUsage::DepthStencilAttachment => (
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            ),
            ImageUsage::Sampled => (
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::VERTEX_SHADER,
            ),
        };

        barriers.insert(Barrier {
            memory_barriers: [vk::MemoryBarrier::builder()
                .src_access_mask(src_access)
                .dst_access_mask(dst_access)
                .build()],
            src_stage,
            dst_stage,
        });
    }

    let mut barriers: Vec<Barrier> = barriers.into_iter().collect();

    let mut i = 0;
    while i < barriers.len() {
        let mut j = i + 1;
        while j < barriers.len() {
            if barriers[j] == barriers[i] {
                barriers.remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }

    barriers
}

/// Helper to create attachment descriptor given operations and an image.
///
/// Returns attachment descriptor.
///
/// Returns `Some` with the clear value if `ops` load operation is `Clear`.
///
/// Returns `Some` with dependency if dependency is needed.
fn create_attachment_descriptor<V: Copy>(
    ops: &Operations<V>,
    state: &ImageState,
    image: &Image,
    dst_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
) -> (
    vk::AttachmentDescription,
    Option<V>,
    Option<vk::SubpassDependency>,
) {
    let mut clear_value = None;
    let mut dependency = None;
    let mut builder = vk::AttachmentDescription::builder()
        .format(image.format())
        .samples(vk::SampleCountFlags::TYPE_1)
        .store_op(if ops.store {
            vk::AttachmentStoreOp::STORE
        } else {
            vk::AttachmentStoreOp::DONT_CARE
        })
        .stencil_store_op(if ops.store {
            vk::AttachmentStoreOp::STORE
        } else {
            vk::AttachmentStoreOp::DONT_CARE
        });

    match ops.load {
        LoadOp::Clear(v) => {
            clear_value = Some(v);

            builder = builder
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .stencil_load_op(vk::AttachmentLoadOp::CLEAR);
        }
        LoadOp::Load => {
            builder = builder
                .load_op(vk::AttachmentLoadOp::LOAD)
                .stencil_load_op(vk::AttachmentLoadOp::LOAD);
        }
        LoadOp::DontCare => {
            builder = builder
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE);
        }
    }

    match state.last.0 {
        ImageUsage::Unused => {
            builder = builder.initial_layout(vk::ImageLayout::UNDEFINED);
        }
        ImageUsage::ColorAttachment => {
            builder = builder.initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

            dependency = Some(
                vk::SubpassDependency::builder()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                    .dst_stage_mask(dst_stage)
                    .src_access_mask(
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    )
                    .dst_access_mask(dst_access)
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .build(),
            );
        }
        ImageUsage::DepthStencilAttachment => {
            builder = builder.initial_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

            dependency = Some(
                vk::SubpassDependency::builder()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(
                        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    )
                    .dst_stage_mask(dst_stage)
                    .src_access_mask(
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )
                    .dst_access_mask(dst_access)
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .build(),
            );
        }
        ImageUsage::Sampled => {
            builder = builder.initial_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

            dependency = Some(
                vk::SubpassDependency::builder()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(vk::PipelineStageFlags::ALL_GRAPHICS)
                    .dst_stage_mask(dst_stage)
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(dst_access)
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .build(),
            );
        }
        ImageUsage::Compute => {
            builder = builder.initial_layout(vk::ImageLayout::GENERAL);

            dependency = Some(
                vk::SubpassDependency::builder()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(vk::PipelineStageFlags::COMPUTE_SHADER)
                    .dst_stage_mask(dst_stage)
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(dst_access)
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .build(),
            );
        }
    }

    // TODO: Do we need dependencies, or should we do barriers manually?
    dependency = None;

    match state.next.0 {
        ImageUsage::Unused => {
            builder = builder.final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        }
        ImageUsage::ColorAttachment => {
            builder = builder.final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }
        ImageUsage::DepthStencilAttachment => {
            builder = builder.final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        }
        ImageUsage::Sampled => {
            builder = builder.final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        }
        ImageUsage::Compute => {
            builder = builder.final_layout(vk::ImageLayout::GENERAL);
        }
    }

    (builder.build(), clear_value, dependency)
}
