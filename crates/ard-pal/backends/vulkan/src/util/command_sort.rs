use api::{
    buffer::Buffer,
    command_buffer::{
        BlitDestination, BlitSource, BufferCubeMapCopy, BufferTextureCopy, Command,
        CopyBufferToBuffer,
    },
    cube_map::CubeMap,
    descriptor_set::DescriptorSet,
    render_pass::{ColorAttachmentSource, RenderPassDescriptor},
    texture::{Blit, Texture},
    types::{BufferUsage, CubeFace, LoadOp, QueueType, SharingMode, StoreOp, TextureUsage},
};
use ash::{
    vk::{self, AccessFlags2, ImageLayout},
    Device,
};
use rustc_hash::FxHashMap;
use std::ops::Range;

use crate::{
    descriptor_set::BoundValue,
    util::usage2::{GlobalBufferUsage, PipelineBarrier},
    QueueFamilyIndices,
};

use super::{
    semaphores::{SemaphoreTracker, WaitInfo},
    usage2::{
        BufferRegion, GlobalImageUsage, GlobalResourceUsage, GlobalSetUsage, ImageRegion,
        QueueUsage, SubResourceUsage,
    },
};

#[derive(Default)]
pub(crate) struct CommandSorting {
    /// Indices of the commands to execute next.
    next_commands: Vec<usize>,
    /// For each command, we keep track of the neccessary barriers.
    commands: Vec<CommandInfo>,
    /// All memory barriers.
    memory_barriers: Vec<MemoryBarrier>,
    /// All buffer memory barriers.
    buffer_barriers: Vec<vk::BufferMemoryBarrier2>,
    /// All image memory barriers.
    image_barriers: Vec<vk::ImageMemoryBarrier2>,
}

unsafe impl Send for CommandSorting {}
unsafe impl Sync for CommandSorting {}

pub(crate) struct CommandSortingInfo<'a> {
    pub global: &'a mut GlobalResourceUsage,
    pub commands: &'a [Command<'a, crate::VulkanBackend>],
    pub semaphores: &'a mut SemaphoreTracker,
    pub queue_families: &'a QueueFamilyIndices,
    pub queue: QueueType,
    pub is_async: bool,
    pub timeline_value: u64,
    /// Timeline value for each queue type. u64::MAX indicates queue type is unused.
    pub wait_queues: [Option<u64>; 4],
}

#[derive(Default)]
struct CommandInfo {
    pub dependents: Vec<usize>,
    pub dependency_count: usize,
    pub images: Range<usize>,
    pub buffers: Range<usize>,
    pub memory: Range<usize>,
}

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct MemoryBarrier {
    src_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_stage: vk::PipelineStageFlags2,
    dst_access: vk::AccessFlags2,
}

impl CommandSorting {
    pub fn create_dag(&mut self, info: &mut CommandSortingInfo) {
        self.next_commands.clear();
        self.memory_barriers.clear();
        self.buffer_barriers.clear();
        self.image_barriers.clear();

        if info.commands.len() > self.commands.len() {
            self.commands
                .resize_with(info.commands.len(), CommandInfo::default);
        }

        self.commands.iter_mut().for_each(|command| {
            command.dependency_count = 0;
            command.dependents.clear();
            command.buffers = Range::default();
            command.images = Range::default();
            command.memory = Range::default();
        });

        let mut i = 0;
        while i < info.commands.len() {
            let command = &info.commands[i];

            let old_memory_barrier_count = self.memory_barriers.len();
            let old_buffer_barrier_count = self.buffer_barriers.len();
            let old_image_barrier_count = self.image_barriers.len();

            let new_i = self.inspect_command(info, command, i);

            let command = &mut self.commands[i];
            command.buffers = old_buffer_barrier_count..self.buffer_barriers.len();
            command.memory = old_memory_barrier_count..self.memory_barriers.len();
            command.images = old_image_barrier_count..self.image_barriers.len();

            if command.dependency_count == 0 {
                self.next_commands.push(i);
            }

            i = new_i;
        }
    }

    pub unsafe fn execute_commands<'a>(
        &'a mut self,
        device: &Device,
        cb: vk::CommandBuffer,
        commands: &[Command<'a, crate::VulkanBackend>],
        mut exec: impl FnMut(vk::CommandBuffer, &Device, usize, &[Command<'a, crate::VulkanBackend>]),
    ) {
        let mut next_commands = std::mem::take(&mut self.next_commands);

        let mut memory_barriers = Vec::default();
        let mut memory_barriers_map = FxHashMap::default();
        let mut buffer_barriers = Vec::default();
        let mut image_barriers = Vec::default();

        while !next_commands.is_empty() {
            memory_barriers.clear();
            memory_barriers_map.clear();
            buffer_barriers.clear();
            image_barriers.clear();

            // Merge all generated barriers
            for command_idx in &next_commands {
                let command = &self.commands[*command_idx];

                for i in command.memory.clone() {
                    let barrier = &self.memory_barriers[i];

                    let entry = memory_barriers_map
                        .entry((barrier.dst_access, barrier.dst_stage))
                        .or_insert((vk::AccessFlags2::empty(), vk::PipelineStageFlags2::empty()));

                    entry.0 |= barrier.src_access;
                    entry.1 |= barrier.src_stage;
                }

                for i in command.buffers.clone() {
                    buffer_barriers.push(self.buffer_barriers[i]);
                }

                for i in command.images.clone() {
                    image_barriers.push(self.image_barriers[i]);
                }
            }

            // Execute the barrier if needed
            if !memory_barriers_map.is_empty()
                || !buffer_barriers.is_empty()
                || !image_barriers.is_empty()
            {
                memory_barriers = memory_barriers_map
                    .iter()
                    .map(|(dst, src)| {
                        vk::MemoryBarrier2::builder()
                            .dst_access_mask(dst.0)
                            .dst_stage_mask(dst.1)
                            .src_access_mask(src.0)
                            .src_stage_mask(src.1)
                            .build()
                    })
                    .collect();

                let dep = vk::DependencyInfo::builder()
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .memory_barriers(&memory_barriers)
                    .buffer_memory_barriers(&buffer_barriers)
                    .image_memory_barriers(&image_barriers)
                    .build();

                device.cmd_pipeline_barrier2(cb, &dep);
            }

            // Execute each command
            let mut new_commands = Vec::default();
            for command_idx in next_commands.drain(..) {
                // Execute
                exec(cb, device, command_idx, commands);

                // Update next commands
                let mut deps = std::mem::take(&mut self.commands[command_idx].dependents);
                for dep in deps.drain(..) {
                    let dep_command = &mut self.commands[dep];
                    dep_command.dependency_count -= 1;

                    if dep_command.dependency_count == 0 {
                        new_commands.push(dep);
                    }
                }
                self.commands[command_idx].dependents = deps;
            }
            next_commands = new_commands;
        }
    }

    /// Inspects a command for resource usages.
    ///
    /// Returns the next index to inspect.
    fn inspect_command(
        &mut self,
        info: &mut CommandSortingInfo,
        command: &Command<'_, crate::VulkanBackend>,
        command_idx: usize,
    ) -> usize {
        match command {
            Command::BeginRenderPass(desc, _) => self.inspect_render_pass(info, command_idx, desc),
            Command::BeginComputePass(_, _) => self.inspect_compute_pass(info, command_idx),
            Command::TransferBufferOwnership {
                buffer,
                array_element,
                new_queue,
                usage_hint,
            } => {
                self.inspect_transfer_buffer_ownership(
                    info,
                    command_idx,
                    buffer,
                    *array_element,
                    *new_queue,
                    *usage_hint,
                );
                command_idx + 1
            }
            Command::TransferTextureOwnership {
                texture,
                array_element,
                base_mip,
                mip_count,
                new_queue,
                ..
            } => {
                self.inspect_transfer_texture_ownership(
                    info,
                    command_idx,
                    texture,
                    *array_element as u32,
                    *base_mip as u32,
                    *mip_count as u32,
                    *new_queue,
                );
                command_idx + 1
            }
            Command::TransferCubeMapOwnership {
                cube_map,
                array_element,
                base_mip,
                mip_count,
                face,
                new_queue,
                usage_hint,
            } => {
                self.inspect_transfer_cube_map_ownership(
                    info,
                    command_idx,
                    cube_map,
                    *face,
                    *array_element as u32,
                    *base_mip as u32,
                    *mip_count as u32,
                    *new_queue,
                    *usage_hint,
                );
                command_idx + 1
            }
            Command::CopyBufferToBuffer(copy) => {
                self.inspect_copy_buffer_to_buffer(info, command_idx, copy);
                command_idx + 1
            }
            Command::CopyBufferToTexture {
                buffer,
                texture,
                copy,
            } => {
                self.inspect_copy_buffer_to_texture(info, command_idx, buffer, texture, copy);
                command_idx + 1
            }
            Command::CopyTextureToBuffer {
                buffer,
                texture,
                copy,
            } => {
                self.inspect_copy_texture_to_buffer(info, command_idx, texture, buffer, copy);
                command_idx + 1
            }
            Command::CopyBufferToCubeMap {
                buffer,
                cube_map,
                copy,
            } => {
                self.inspect_copy_buffer_to_cube_map(info, command_idx, buffer, cube_map, copy);
                command_idx + 1
            }
            Command::CopyCubeMapToBuffer {
                cube_map,
                buffer,
                copy,
            } => {
                self.inspect_copy_cube_map_to_buffer(info, command_idx, cube_map, buffer, copy);
                command_idx + 1
            }
            Command::Blit { src, dst, blit, .. } => {
                self.inspect_blit(info, command_idx, src, dst, blit);
                command_idx + 1
            }
            Command::SetTextureUsage {
                tex,
                new_usage,
                array_elem,
                base_mip,
                mip_count,
            } => {
                self.inspect_set_texture_usage(
                    info,
                    command_idx,
                    tex,
                    *new_usage,
                    *array_elem,
                    *base_mip,
                    *mip_count,
                );
                command_idx + 1
            }
            _ => command_idx + 1,
        }
    }

    fn inspect_render_pass(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        desc: &RenderPassDescriptor<'_, crate::VulkanBackend>,
    ) -> usize {
        for attachment in &desc.color_attachments {
            self.inspect_color_attachment(
                info,
                command_idx,
                &attachment.load_op,
                &attachment.source,
            );
        }

        for attachment in &desc.color_resolve_attachments {
            self.inspect_color_attachment(info, command_idx, &attachment.load_op, &attachment.dst);
        }

        if let Some(attachment) = &desc.depth_stencil_attachment {
            self.inspect_depth_stencil_attachment(
                info,
                command_idx,
                attachment.store_op,
                &attachment.load_op,
                &attachment.texture,
                attachment.array_element as u32,
                attachment.mip_level as u32,
                false,
            );
        }

        if let Some(attachment) = &desc.depth_stencil_resolve_attachment {
            self.inspect_depth_stencil_attachment(
                info,
                command_idx,
                attachment.store_op,
                &attachment.load_op,
                &attachment.dst,
                attachment.array_element as u32,
                attachment.mip_level as u32,
                true,
            );
        }

        let mut i = command_idx;
        loop {
            i += 1;
            if !self.inspect_render_pass_command(info, command_idx, &info.commands[i]) {
                break;
            }
        }
        i + 1
    }

    fn inspect_compute_pass(&mut self, info: &mut CommandSortingInfo, command_idx: usize) -> usize {
        let mut i = command_idx;
        loop {
            i += 1;
            if !self.inspect_compute_pass_command(info, command_idx, &info.commands[i]) {
                break;
            }
        }
        i + 1
    }

    fn inspect_color_attachment(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        load_op: &LoadOp,
        source: &ColorAttachmentSource<'_, crate::VulkanBackend>,
    ) {
        let (image_region, image, initial_layout, final_layout) = match source {
            ColorAttachmentSource::SurfaceImage(image) => {
                let semaphores = image.internal().semaphores();
                info.semaphores
                    .register_signal(semaphores.presentable, None);
                info.semaphores.register_wait(
                    semaphores.available,
                    WaitInfo {
                        value: None,
                        stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    },
                );

                (
                    ImageRegion {
                        id: image.internal().id(),
                        array_elem: 0,
                        base_mip_level: 0,
                        mip_count: 1,
                    },
                    image.internal().image(),
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                )
            }
            ColorAttachmentSource::Texture {
                texture,
                array_element,
                mip_level,
            } => (
                ImageRegion {
                    id: texture.internal().id,
                    array_elem: *array_element as u32,
                    base_mip_level: *mip_level as u32,
                    mip_count: 1,
                },
                texture.internal().image,
                match load_op {
                    LoadOp::DontCare => vk::ImageLayout::UNDEFINED,
                    LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    LoadOp::Clear(_) => vk::ImageLayout::UNDEFINED,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            ColorAttachmentSource::CubeMap {
                cube_map,
                array_element,
                face,
                mip_level,
            } => (
                ImageRegion {
                    id: cube_map.internal().id,
                    array_elem: crate::cube_map::CubeMap::to_array_elem(*array_element, *face)
                        as u32,
                    base_mip_level: *mip_level as u32,
                    mip_count: 1,
                },
                cube_map.internal().image,
                match load_op {
                    LoadOp::DontCare => vk::ImageLayout::UNDEFINED,
                    LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    LoadOp::Clear(_) => vk::ImageLayout::UNDEFINED,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
        };

        let mut new_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            },
            layout: final_layout,
        };

        let mut old_usage = [GlobalImageUsage::default()];
        info.global
            .use_image(&image_region, &new_usage, &mut old_usage);

        new_usage.layout = initial_layout;

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_usage[0],
            &new_usage,
            image,
            vk::ImageAspectFlags::COLOR,
            image_region.array_elem,
            image_region.base_mip_level,
        );

        self.dependency_check(
            old_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_depth_stencil_attachment(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        store_op: StoreOp,
        load_op: &LoadOp,
        texture: &Texture<crate::VulkanBackend>,
        array_elem: u32,
        mip_level: u32,
        is_resolve_attachment: bool,
    ) {
        let image_region = ImageRegion {
            id: texture.internal().id,
            array_elem,
            base_mip_level: mip_level,
            mip_count: 1,
        };

        let final_layout = crate::util::depth_store_op_to_layout(store_op);

        let mut new_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),

            sub_resource: if is_resolve_attachment {
                // NOTE: This causes a false positive in validation because we're using a depth
                // stencil layout, but writing to the image during the color attachment output
                // stage. This is a byproduct of how multisample resolve works, and is safe
                // to ignore.
                SubResourceUsage {
                    access: vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                }
            } else {
                SubResourceUsage {
                    access: match store_op {
                        StoreOp::None => vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                        _ => {
                            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                        }
                    },
                    stage: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                }
            },
            layout: final_layout,
        };

        let mut old_usage = [GlobalImageUsage::default()];
        info.global
            .use_image(&image_region, &new_usage, &mut old_usage);

        new_usage.layout = match load_op {
            LoadOp::DontCare => vk::ImageLayout::UNDEFINED,
            LoadOp::Load => final_layout,
            LoadOp::Clear(_) => vk::ImageLayout::UNDEFINED,
        };

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_usage[0],
            &new_usage,
            texture.internal().image,
            texture.internal().aspect_flags,
            image_region.array_elem,
            image_region.base_mip_level,
        );

        self.dependency_check(
            old_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_render_pass_command(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        rp_command: &Command<'_, crate::VulkanBackend>,
    ) -> bool {
        match rp_command {
            Command::BindVertexBuffers { binds, .. } => {
                for bind in binds {
                    let new_usage = GlobalBufferUsage {
                        queue: Some(QueueUsage {
                            queue: info.queue,
                            timeline_value: info.timeline_value,
                            command_idx,
                            is_async: info.is_async,
                        }),
                        sub_resource: SubResourceUsage {
                            access: vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
                            stage: vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT,
                        },
                    };

                    let old_usage = info.global.use_buffer(
                        &BufferRegion {
                            id: bind.buffer.internal().id,
                            array_elem: bind.array_element as u32,
                        },
                        &new_usage,
                    );

                    self.buffer_barrier_check(
                        info.queue_families,
                        info.queue_families.to_index(info.queue),
                        &old_usage,
                        &new_usage,
                        bind.buffer.internal().buffer,
                        bind.buffer.internal().sharing_mode,
                        bind.buffer.internal().aligned_size,
                        bind.buffer.internal().offset(bind.array_element),
                    );

                    self.dependency_check(
                        old_usage.queue.as_ref(),
                        command_idx,
                        &mut info.wait_queues,
                        (info.queue, info.timeline_value),
                    );
                }

                true
            }
            Command::BindIndexBuffer {
                buffer,
                array_element,
                ..
            } => {
                let new_usage = GlobalBufferUsage {
                    queue: Some(QueueUsage {
                        queue: info.queue,
                        timeline_value: info.timeline_value,
                        command_idx,
                        is_async: info.is_async,
                    }),
                    sub_resource: SubResourceUsage {
                        access: vk::AccessFlags2::INDEX_READ,
                        stage: vk::PipelineStageFlags2::INDEX_INPUT,
                    },
                };

                let old_usage = info.global.use_buffer(
                    &BufferRegion {
                        id: buffer.internal().id,
                        array_elem: *array_element as u32,
                    },
                    &new_usage,
                );

                self.buffer_barrier_check(
                    info.queue_families,
                    info.queue_families.to_index(info.queue),
                    &old_usage,
                    &new_usage,
                    buffer.internal().buffer,
                    buffer.internal().sharing_mode,
                    buffer.internal().aligned_size,
                    buffer.internal().offset(*array_element),
                );

                self.dependency_check(
                    old_usage.queue.as_ref(),
                    command_idx,
                    &mut info.wait_queues,
                    (info.queue, info.timeline_value),
                );

                true
            }
            Command::BindDescriptorSets { sets, .. } => {
                for set in sets {
                    self.inspect_descriptor_set(
                        info,
                        command_idx,
                        vk::PipelineStageFlags2::VERTEX_SHADER
                            | vk::PipelineStageFlags2::FRAGMENT_SHADER,
                        set,
                    );
                }
                true
            }
            Command::DrawIndexedIndirect {
                buffer,
                array_element,
                ..
            } => {
                let new_usage = GlobalBufferUsage {
                    queue: Some(QueueUsage {
                        queue: info.queue,
                        timeline_value: info.timeline_value,
                        command_idx,
                        is_async: info.is_async,
                    }),
                    sub_resource: SubResourceUsage {
                        access: vk::AccessFlags2::INDIRECT_COMMAND_READ,
                        stage: vk::PipelineStageFlags2::DRAW_INDIRECT,
                    },
                };

                let old_usage = info.global.use_buffer(
                    &BufferRegion {
                        id: buffer.internal().id,
                        array_elem: *array_element as u32,
                    },
                    &new_usage,
                );

                self.buffer_barrier_check(
                    info.queue_families,
                    info.queue_families.to_index(info.queue),
                    &old_usage,
                    &new_usage,
                    buffer.internal().buffer,
                    buffer.internal().sharing_mode,
                    buffer.internal().aligned_size,
                    buffer.internal().offset(*array_element),
                );

                self.dependency_check(
                    old_usage.queue.as_ref(),
                    command_idx,
                    &mut info.wait_queues,
                    (info.queue, info.timeline_value),
                );

                true
            }
            Command::DrawIndexedIndirectCount {
                draw_buffer,
                draw_array_element,
                count_buffer,
                count_array_element,
                ..
            } => {
                let new_usage = GlobalBufferUsage {
                    queue: Some(QueueUsage {
                        queue: info.queue,
                        timeline_value: info.timeline_value,
                        command_idx,
                        is_async: info.is_async,
                    }),
                    sub_resource: SubResourceUsage {
                        access: vk::AccessFlags2::INDIRECT_COMMAND_READ,
                        stage: vk::PipelineStageFlags2::DRAW_INDIRECT,
                    },
                };

                let old_draw_usage = info.global.use_buffer(
                    &BufferRegion {
                        id: draw_buffer.internal().id,
                        array_elem: *draw_array_element as u32,
                    },
                    &new_usage,
                );

                let old_count_usage = info.global.use_buffer(
                    &BufferRegion {
                        id: count_buffer.internal().id,
                        array_elem: *count_array_element as u32,
                    },
                    &new_usage,
                );

                self.buffer_barrier_check(
                    info.queue_families,
                    info.queue_families.to_index(info.queue),
                    &old_draw_usage,
                    &new_usage,
                    draw_buffer.internal().buffer,
                    draw_buffer.internal().sharing_mode,
                    draw_buffer.internal().aligned_size,
                    draw_buffer.internal().offset(*draw_array_element),
                );

                self.dependency_check(
                    old_draw_usage.queue.as_ref(),
                    command_idx,
                    &mut info.wait_queues,
                    (info.queue, info.timeline_value),
                );

                self.buffer_barrier_check(
                    info.queue_families,
                    info.queue_families.to_index(info.queue),
                    &old_count_usage,
                    &new_usage,
                    count_buffer.internal().buffer,
                    count_buffer.internal().sharing_mode,
                    count_buffer.internal().aligned_size,
                    count_buffer.internal().offset(*count_array_element),
                );

                self.dependency_check(
                    old_count_usage.queue.as_ref(),
                    command_idx,
                    &mut info.wait_queues,
                    (info.queue, info.timeline_value),
                );

                true
            }
            Command::EndRenderPass(_) => false,
            _ => true,
        }
    }

    fn inspect_compute_pass_command(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        cp_command: &Command<'_, crate::VulkanBackend>,
    ) -> bool {
        match cp_command {
            Command::BindDescriptorSets { sets, .. } => {
                for set in sets {
                    self.inspect_descriptor_set(
                        info,
                        command_idx,
                        vk::PipelineStageFlags2::COMPUTE_SHADER,
                        set,
                    );
                }
                true
            }
            Command::EndComputePass(_, _, _, _) => false,
            _ => true,
        }
    }

    fn inspect_descriptor_set(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        valid_stages: vk::PipelineStageFlags2,
        set: &DescriptorSet<crate::VulkanBackend>,
    ) {
        // Use set
        let new_usage = GlobalSetUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
        };

        info.global.use_set(set.internal().id, &new_usage);

        let mut out_usages: [GlobalImageUsage; 14] = Default::default();

        // Use set resources
        set.internal()
            .bound
            .iter()
            .flat_map(|binding| binding.iter().flatten())
            .for_each(|elem| match &elem.value {
                BoundValue::UniformBuffer {
                    buffer,
                    id,
                    array_element,
                    aligned_size,
                    sharing_mode,
                    ..
                } => {
                    let new_usage = GlobalBufferUsage {
                        queue: Some(QueueUsage {
                            queue: info.queue,
                            timeline_value: info.timeline_value,
                            command_idx,
                            is_async: info.is_async,
                        }),
                        sub_resource: SubResourceUsage {
                            access: elem.access,
                            stage: elem.stage & valid_stages,
                        },
                    };

                    let old_usage = info.global.use_buffer(
                        &BufferRegion {
                            id: *id,
                            array_elem: *array_element as u32,
                        },
                        &new_usage,
                    );

                    self.buffer_barrier_check(
                        info.queue_families,
                        info.queue_families.to_index(info.queue),
                        &old_usage,
                        &new_usage,
                        *buffer,
                        *sharing_mode,
                        *aligned_size as u64,
                        (*aligned_size * *array_element) as u64,
                    );

                    self.dependency_check(
                        old_usage.queue.as_ref(),
                        command_idx,
                        &mut info.wait_queues,
                        (info.queue, info.timeline_value),
                    );
                }
                BoundValue::StorageBuffer {
                    buffer,
                    id,
                    array_element,
                    aligned_size,
                    sharing_mode,
                    ..
                } => {
                    let new_usage = GlobalBufferUsage {
                        queue: Some(QueueUsage {
                            queue: info.queue,
                            timeline_value: info.timeline_value,
                            command_idx,
                            is_async: info.is_async,
                        }),
                        sub_resource: SubResourceUsage {
                            access: elem.access,
                            stage: elem.stage & valid_stages,
                        },
                    };

                    let old_usage = info.global.use_buffer(
                        &BufferRegion {
                            id: *id,
                            array_elem: *array_element as u32,
                        },
                        &new_usage,
                    );

                    self.buffer_barrier_check(
                        info.queue_families,
                        info.queue_families.to_index(info.queue),
                        &old_usage,
                        &new_usage,
                        *buffer,
                        *sharing_mode,
                        *aligned_size as u64,
                        (*aligned_size * *array_element) as u64,
                    );

                    self.dependency_check(
                        old_usage.queue.as_ref(),
                        command_idx,
                        &mut info.wait_queues,
                        (info.queue, info.timeline_value),
                    );
                }
                BoundValue::StorageImage {
                    image,
                    id,
                    aspect_mask,
                    mip,
                    array_element,
                    ..
                } => {
                    let image_region = ImageRegion {
                        id: *id,
                        array_elem: *array_element as u32,
                        base_mip_level: *mip as u32,
                        mip_count: 1,
                    };

                    let new_src_usage = GlobalImageUsage {
                        queue: Some(QueueUsage {
                            queue: info.queue,
                            timeline_value: info.timeline_value,
                            command_idx,
                            is_async: info.is_async,
                        }),
                        sub_resource: SubResourceUsage {
                            access: elem.access,
                            stage: elem.stage & valid_stages,
                        },
                        layout: ImageLayout::GENERAL,
                    };

                    let mut old_src_usage = [GlobalImageUsage::default()];
                    info.global
                        .use_image(&image_region, &new_src_usage, &mut old_src_usage);

                    self.image_barrier_check(
                        info.queue_families,
                        info.queue_families.to_index(info.queue),
                        &old_src_usage[0],
                        &new_src_usage,
                        *image,
                        *aspect_mask,
                        *array_element as u32,
                        *mip as u32,
                    );

                    self.dependency_check(
                        old_src_usage[0].queue.as_ref(),
                        command_idx,
                        &mut info.wait_queues,
                        (info.queue, info.timeline_value),
                    );
                }
                BoundValue::Texture {
                    image,
                    id,
                    aspect_mask,
                    base_mip,
                    mip_count,
                    array_element,
                    ..
                } => {
                    let image_region = ImageRegion {
                        id: *id,
                        array_elem: *array_element as u32,
                        base_mip_level: *base_mip,
                        mip_count: *mip_count,
                    };

                    let new_src_usage = GlobalImageUsage {
                        queue: Some(QueueUsage {
                            queue: info.queue,
                            timeline_value: info.timeline_value,
                            command_idx,
                            is_async: info.is_async,
                        }),
                        sub_resource: SubResourceUsage {
                            access: elem.access,
                            stage: elem.stage & valid_stages,
                        },
                        layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    };

                    info.global
                        .use_image(&image_region, &new_src_usage, &mut out_usages);

                    for (i, mip) in (*base_mip..(*base_mip + *mip_count))
                        .into_iter()
                        .enumerate()
                    {
                        self.image_barrier_check(
                            info.queue_families,
                            info.queue_families.to_index(info.queue),
                            &out_usages[i],
                            &new_src_usage,
                            *image,
                            *aspect_mask,
                            *array_element as u32,
                            mip,
                        );

                        self.dependency_check(
                            out_usages[i].queue.as_ref(),
                            command_idx,
                            &mut info.wait_queues,
                            (info.queue, info.timeline_value),
                        );
                    }
                }
                BoundValue::CubeMap {
                    image,
                    id,
                    aspect_mask,
                    base_mip,
                    mip_count,
                    array_element,
                    ..
                } => {
                    for face in 0..6 {
                        let array_element = ((*array_element * 6) + face) as u32;

                        let image_region = ImageRegion {
                            id: *id,
                            array_elem: array_element,
                            base_mip_level: *base_mip,
                            mip_count: *mip_count,
                        };

                        let new_src_usage = GlobalImageUsage {
                            queue: Some(QueueUsage {
                                queue: info.queue,
                                timeline_value: info.timeline_value,
                                command_idx,
                                is_async: info.is_async,
                            }),
                            sub_resource: SubResourceUsage {
                                access: elem.access,
                                stage: elem.stage & valid_stages,
                            },
                            layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        };

                        info.global
                            .use_image(&image_region, &new_src_usage, &mut out_usages);

                        for (i, mip) in (*base_mip..(*base_mip + *mip_count))
                            .into_iter()
                            .enumerate()
                        {
                            self.image_barrier_check(
                                info.queue_families,
                                info.queue_families.to_index(info.queue),
                                &out_usages[i],
                                &new_src_usage,
                                *image,
                                *aspect_mask,
                                array_element,
                                mip,
                            );

                            self.dependency_check(
                                out_usages[i].queue.as_ref(),
                                command_idx,
                                &mut info.wait_queues,
                                (info.queue, info.timeline_value),
                            );
                        }
                    }
                }
            });
    }

    fn inspect_copy_buffer_to_buffer(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        copy: &CopyBufferToBuffer<crate::VulkanBackend>,
    ) {
        let new_src_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_READ,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let new_dst_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let old_src_usage = info.global.use_buffer(
            &BufferRegion {
                id: copy.src.internal().id,
                array_elem: copy.src_array_element as u32,
            },
            &new_src_usage,
        );

        let old_dst_usage = info.global.use_buffer(
            &BufferRegion {
                id: copy.dst.internal().id,
                array_elem: copy.dst_array_element as u32,
            },
            &new_dst_usage,
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_src_usage,
            &new_src_usage,
            copy.src.internal().buffer,
            copy.src.internal().sharing_mode,
            copy.src.internal().aligned_size,
            copy.src.internal().offset(copy.src_array_element),
        );

        self.dependency_check(
            old_src_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_dst_usage,
            &new_dst_usage,
            copy.dst.internal().buffer,
            copy.dst.internal().sharing_mode,
            copy.dst.internal().aligned_size,
            copy.dst.internal().offset(copy.dst_array_element),
        );

        self.dependency_check(
            old_dst_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_copy_buffer_to_texture(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        src: &Buffer<crate::VulkanBackend>,
        dst: &Texture<crate::VulkanBackend>,
        copy: &BufferTextureCopy,
    ) {
        let new_src_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_READ,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let new_dst_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
            layout: ImageLayout::TRANSFER_DST_OPTIMAL,
        };

        let old_src_usage = info.global.use_buffer(
            &BufferRegion {
                id: src.internal().id,
                array_elem: copy.buffer_array_element as u32,
            },
            &new_src_usage,
        );

        let mut old_dst_usage = [GlobalImageUsage::default()];
        info.global.use_image(
            &ImageRegion {
                id: dst.internal().id,
                array_elem: copy.texture_array_element as u32,
                base_mip_level: copy.texture_mip_level as u32,
                mip_count: 1,
            },
            &new_dst_usage,
            &mut old_dst_usage,
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_src_usage,
            &new_src_usage,
            src.internal().buffer,
            src.internal().sharing_mode,
            src.internal().aligned_size,
            src.internal().offset(copy.buffer_array_element),
        );

        self.dependency_check(
            old_src_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_dst_usage[0],
            &new_dst_usage,
            dst.internal().image,
            dst.internal().aspect_flags,
            copy.texture_array_element as u32,
            copy.texture_mip_level as u32,
        );

        self.dependency_check(
            old_dst_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_copy_texture_to_buffer(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        src: &Texture<crate::VulkanBackend>,
        dst: &Buffer<crate::VulkanBackend>,
        copy: &BufferTextureCopy,
    ) {
        let image_region = ImageRegion {
            id: dst.internal().id,
            array_elem: copy.texture_array_element as u32,
            base_mip_level: copy.texture_mip_level as u32,
            mip_count: 1,
        };

        let new_src_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_READ,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
            layout: ImageLayout::TRANSFER_SRC_OPTIMAL,
        };

        let new_dst_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let mut old_src_usage = [GlobalImageUsage::default()];
        info.global
            .use_image(&image_region, &new_src_usage, &mut old_src_usage);

        let old_dst_usage = info.global.use_buffer(
            &BufferRegion {
                id: dst.internal().id,
                array_elem: copy.buffer_array_element as u32,
            },
            &new_dst_usage,
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_dst_usage,
            &new_dst_usage,
            dst.internal().buffer,
            dst.internal().sharing_mode,
            dst.internal().aligned_size,
            dst.internal().offset(copy.buffer_array_element),
        );

        self.dependency_check(
            old_dst_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_src_usage[0],
            &new_src_usage,
            src.internal().image,
            src.internal().aspect_flags,
            copy.texture_array_element as u32,
            copy.texture_mip_level as u32,
        );

        self.dependency_check(
            old_src_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_copy_buffer_to_cube_map(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        src: &Buffer<crate::VulkanBackend>,
        dst: &CubeMap<crate::VulkanBackend>,
        copy: &BufferCubeMapCopy,
    ) {
        let new_src_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_READ,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let new_dst_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
            layout: ImageLayout::TRANSFER_DST_OPTIMAL,
        };

        let old_src_usage = info.global.use_buffer(
            &BufferRegion {
                id: src.internal().id,
                array_elem: copy.buffer_array_element as u32,
            },
            &new_src_usage,
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_src_usage,
            &new_src_usage,
            src.internal().buffer,
            src.internal().sharing_mode,
            src.internal().aligned_size,
            src.internal().offset(copy.buffer_array_element),
        );

        self.dependency_check(
            old_src_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        for i in 0..6 {
            let array_elem = ((copy.cube_map_array_element * 6) + i) as u32;
            let mut old_dst_usage = [GlobalImageUsage::default()];
            info.global.use_image(
                &ImageRegion {
                    id: dst.internal().id,
                    array_elem,
                    base_mip_level: copy.cube_map_mip_level as u32,
                    mip_count: 1,
                },
                &new_dst_usage,
                &mut old_dst_usage,
            );

            self.image_barrier_check(
                info.queue_families,
                info.queue_families.to_index(info.queue),
                &old_dst_usage[0],
                &new_dst_usage,
                dst.internal().image,
                dst.internal().aspect_flags,
                array_elem,
                copy.cube_map_mip_level as u32,
            );

            self.dependency_check(
                old_dst_usage[0].queue.as_ref(),
                command_idx,
                &mut info.wait_queues,
                (info.queue, info.timeline_value),
            );
        }
    }

    fn inspect_copy_cube_map_to_buffer(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        src: &CubeMap<crate::VulkanBackend>,
        dst: &Buffer<crate::VulkanBackend>,
        copy: &BufferCubeMapCopy,
    ) {
        for i in 0..6 {
            let array_elem = ((copy.cube_map_array_element * 6) + i) as u32;
            let image_region = ImageRegion {
                id: dst.internal().id,
                array_elem,
                base_mip_level: copy.cube_map_mip_level as u32,
                mip_count: 1,
            };

            let new_src_usage = GlobalImageUsage {
                queue: Some(QueueUsage {
                    queue: info.queue,
                    timeline_value: info.timeline_value,
                    command_idx,
                    is_async: info.is_async,
                }),
                sub_resource: SubResourceUsage {
                    access: vk::AccessFlags2::TRANSFER_READ,
                    stage: vk::PipelineStageFlags2::TRANSFER,
                },
                layout: ImageLayout::TRANSFER_SRC_OPTIMAL,
            };

            let mut old_src_usage = [GlobalImageUsage::default()];
            info.global
                .use_image(&image_region, &new_src_usage, &mut old_src_usage);

            self.image_barrier_check(
                info.queue_families,
                info.queue_families.to_index(info.queue),
                &old_src_usage[0],
                &new_src_usage,
                src.internal().image,
                src.internal().aspect_flags,
                array_elem,
                copy.cube_map_mip_level as u32,
            );

            self.dependency_check(
                old_src_usage[0].queue.as_ref(),
                command_idx,
                &mut info.wait_queues,
                (info.queue, info.timeline_value),
            );
        }

        let new_dst_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
        };

        let old_dst_usage = info.global.use_buffer(
            &BufferRegion {
                id: dst.internal().id,
                array_elem: copy.buffer_array_element as u32,
            },
            &new_dst_usage,
        );

        self.buffer_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_dst_usage,
            &new_dst_usage,
            dst.internal().buffer,
            dst.internal().sharing_mode,
            dst.internal().aligned_size,
            dst.internal().offset(copy.buffer_array_element),
        );

        self.dependency_check(
            old_dst_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_blit(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        src: &BlitSource<'_, crate::VulkanBackend>,
        dst: &BlitDestination<'_, crate::VulkanBackend>,
        blit: &Blit,
    ) {
        let (src_image_region, src_image, src_aspect) = match src {
            BlitSource::Texture(tex) => (
                ImageRegion {
                    id: tex.internal().id,
                    array_elem: blit.src_array_element as u32,
                    base_mip_level: blit.src_mip as u32,
                    mip_count: 1,
                },
                tex.internal().image,
                tex.internal().aspect_flags,
            ),
            BlitSource::CubeMap { cube_map, face } => (
                ImageRegion {
                    id: cube_map.internal().id,
                    array_elem: crate::cube_map::CubeMap::to_array_elem(
                        blit.src_array_element as usize,
                        *face,
                    ) as u32,
                    base_mip_level: blit.src_mip as u32,
                    mip_count: 1,
                },
                cube_map.internal().image,
                cube_map.internal().aspect_flags,
            ),
        };

        let (dst_image_region, dst_image, dst_aspect) = match dst {
            BlitDestination::Texture(tex) => (
                ImageRegion {
                    id: tex.internal().id,
                    array_elem: blit.dst_array_element as u32,
                    base_mip_level: blit.dst_mip as u32,
                    mip_count: 1,
                },
                tex.internal().image,
                tex.internal().aspect_flags,
            ),
            BlitDestination::CubeMap { cube_map, face } => (
                ImageRegion {
                    id: cube_map.internal().id,
                    array_elem: crate::cube_map::CubeMap::to_array_elem(
                        blit.dst_array_element as usize,
                        *face,
                    ) as u32,
                    base_mip_level: blit.dst_mip as u32,
                    mip_count: 1,
                },
                cube_map.internal().image,
                cube_map.internal().aspect_flags,
            ),
            BlitDestination::SurfaceImage(tex) => {
                let internal = tex.internal();
                let semaphores = internal.semaphores();

                // Also handle semaphores of the surface image
                info.semaphores
                    .register_signal(semaphores.presentable, None);
                info.semaphores.register_wait(
                    semaphores.available,
                    WaitInfo {
                        value: None,
                        stage: vk::PipelineStageFlags::TRANSFER,
                    },
                );

                (
                    ImageRegion {
                        id: tex.internal().id(),
                        array_elem: blit.dst_array_element as u32,
                        base_mip_level: blit.dst_mip as u32,
                        mip_count: 1,
                    },
                    tex.internal().image(),
                    vk::ImageAspectFlags::COLOR,
                )
            }
        };

        let new_src_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_READ,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
            layout: ImageLayout::TRANSFER_SRC_OPTIMAL,
        };

        let mut old_src_usage = [GlobalImageUsage::default()];
        info.global
            .use_image(&src_image_region, &new_src_usage, &mut old_src_usage);

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_src_usage[0],
            &new_src_usage,
            src_image,
            src_aspect,
            src_image_region.array_elem,
            src_image_region.base_mip_level,
        );

        self.dependency_check(
            old_src_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        let new_dst_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::TRANSFER_WRITE,
                stage: vk::PipelineStageFlags2::TRANSFER,
            },
            layout: ImageLayout::TRANSFER_DST_OPTIMAL,
        };

        let mut old_dst_usage = [GlobalImageUsage::default()];
        info.global
            .use_image(&dst_image_region, &new_dst_usage, &mut old_dst_usage);

        self.image_barrier_check(
            info.queue_families,
            info.queue_families.to_index(info.queue),
            &old_dst_usage[0],
            &new_dst_usage,
            dst_image,
            dst_aspect,
            dst_image_region.array_elem,
            dst_image_region.base_mip_level,
        );

        self.dependency_check(
            old_dst_usage[0].queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );
    }

    fn inspect_set_texture_usage(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        texture: &Texture<crate::VulkanBackend>,
        new_usage: TextureUsage,
        array_element: usize,
        base_mip: u32,
        mip_count: usize,
    ) {
        let layout = match new_usage {
            TextureUsage::COLOR_ATTACHMENT => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            TextureUsage::DEPTH_STENCIL_ATTACHMENT => {
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
            }
            TextureUsage::TRANSFER_DST => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            TextureUsage::TRANSFER_SRC => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            TextureUsage::STORAGE => vk::ImageLayout::GENERAL,
            TextureUsage::SAMPLED => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            _ => unreachable!("command guarantees only one usage"),
        };

        let new_usage = GlobalImageUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::empty(),
                stage: vk::PipelineStageFlags2::empty(),
            },
            layout,
        };

        let image_region = ImageRegion {
            id: texture.internal().id,
            array_elem: array_element as u32,
            base_mip_level: base_mip,
            mip_count: mip_count as u32,
        };

        let mut old_usages = vec![GlobalImageUsage::default(); mip_count];
        info.global
            .use_image(&image_region, &new_usage, &mut old_usages);

        for (i, old_usage) in old_usages.into_iter().enumerate() {
            self.image_barrier_check(
                info.queue_families,
                info.queue_families.to_index(info.queue),
                &old_usage,
                &new_usage,
                texture.internal().image,
                texture.internal().aspect_flags,
                array_element as u32,
                base_mip + i as u32,
            );

            self.dependency_check(
                old_usage.queue.as_ref(),
                command_idx,
                &mut info.wait_queues,
                (info.queue, info.timeline_value),
            );
        }
    }

    fn inspect_transfer_buffer_ownership(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        buffer: &Buffer<crate::VulkanBackend>,
        array_element: usize,
        new_queue: QueueType,
        _usage_hint: Option<BufferUsage>,
    ) {
        // Replace the old queue usage with the new one
        let new_usage = GlobalBufferUsage {
            queue: Some(QueueUsage {
                queue: info.queue,
                timeline_value: info.timeline_value,
                command_idx,
                is_async: info.is_async,
            }),
            // NOTE: This subresource usage is a fake. We're essentially forcing there to be
            // a hard dependency with whatever the last command was.
            sub_resource: SubResourceUsage {
                access: AccessFlags2::MEMORY_WRITE,
                stage: vk::PipelineStageFlags2::empty(),
            },
        };

        let old_usage = info.global.use_buffer(
            &BufferRegion {
                id: buffer.internal().id,
                array_elem: array_element as u32,
            },
            &new_usage,
        );

        let (src_access, src_stage) = (old_usage.sub_resource.access, old_usage.sub_resource.stage);

        self.dependency_check(
            old_usage.queue.as_ref(),
            command_idx,
            &mut info.wait_queues,
            (info.queue, info.timeline_value),
        );

        self.buffer_barriers.push(
            vk::BufferMemoryBarrier2::builder()
                .src_queue_family_index(info.queue_families.to_index(info.queue))
                .src_access_mask(src_access)
                .src_stage_mask(src_stage)
                .dst_queue_family_index(info.queue_families.to_index(new_queue))
                .dst_access_mask(vk::AccessFlags2::empty())
                .dst_stage_mask(vk::PipelineStageFlags2::empty())
                .buffer(buffer.internal().buffer)
                .size(buffer.internal().aligned_size)
                .offset(buffer.internal().offset(array_element))
                .build(),
        );
    }

    fn inspect_transfer_texture_ownership(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        texture: &Texture<crate::VulkanBackend>,
        array_element: u32,
        base_mip: u32,
        mip_count: u32,
        new_queue: QueueType,
    ) {
        let mut old_usages = vec![GlobalImageUsage::default(); mip_count as usize];
        info.global.use_image(
            &ImageRegion {
                id: texture.internal().id,
                array_elem: array_element,
                base_mip_level: base_mip,
                mip_count,
            },
            &GlobalImageUsage {
                queue: Some(QueueUsage {
                    queue: info.queue,
                    timeline_value: info.timeline_value,
                    command_idx,
                    is_async: info.is_async,
                }),
                // NOTE: This subresource usage is a fake. We're essentially forcing there to be
                // a hard dependency with whatever the last command was.
                sub_resource: SubResourceUsage {
                    access: AccessFlags2::MEMORY_WRITE,
                    stage: vk::PipelineStageFlags2::empty(),
                },
                layout: vk::ImageLayout::UNDEFINED,
            },
            &mut old_usages,
        );

        for (i, old_usage) in old_usages.iter_mut().enumerate() {
            let (src_access, src_stage) =
                (old_usage.sub_resource.access, old_usage.sub_resource.stage);

            // If another command used this resource this command buffer, we add it as a dependency
            self.dependency_check(
                old_usage.queue.as_ref(),
                command_idx,
                &mut info.wait_queues,
                (info.queue, info.timeline_value),
            );

            self.image_barriers.push(
                vk::ImageMemoryBarrier2::builder()
                    .src_queue_family_index(info.queue_families.to_index(info.queue))
                    .src_access_mask(src_access)
                    .src_stage_mask(src_stage)
                    .old_layout(old_usage.layout)
                    .dst_queue_family_index(info.queue_families.to_index(new_queue))
                    .dst_access_mask(vk::AccessFlags2::empty())
                    .dst_stage_mask(vk::PipelineStageFlags2::empty())
                    .new_layout(old_usage.layout)
                    .image(texture.internal().image)
                    .subresource_range(
                        vk::ImageSubresourceRange::builder()
                            .aspect_mask(texture.internal().aspect_flags)
                            .base_mip_level(base_mip + i as u32)
                            .level_count(1)
                            .base_array_layer(array_element)
                            .layer_count(1)
                            .build(),
                    )
                    .build(),
            );
        }
    }

    fn inspect_transfer_cube_map_ownership(
        &mut self,
        info: &mut CommandSortingInfo,
        command_idx: usize,
        cube_map: &CubeMap<crate::VulkanBackend>,
        face: CubeFace,
        array_element: u32,
        base_mip: u32,
        mip_count: u32,
        new_queue: QueueType,
        usage_hint: Option<TextureUsage>,
    ) {
        let array_element =
            crate::cube_map::CubeMap::to_array_elem(array_element as usize, face) as u32;

        let mut old_usages = vec![GlobalImageUsage::default(); mip_count as usize];
        info.global.use_image(
            &ImageRegion {
                id: cube_map.internal().id,
                array_elem: array_element,
                base_mip_level: base_mip,
                mip_count,
            },
            &GlobalImageUsage {
                queue: Some(QueueUsage {
                    queue: info.queue,
                    timeline_value: info.timeline_value,
                    command_idx,
                    is_async: info.is_async,
                }),
                // NOTE: This subresource usage is a fake. We're essentially forcing there to be
                // a hard dependency with whatever the last command was.
                sub_resource: SubResourceUsage {
                    access: AccessFlags2::MEMORY_WRITE,
                    stage: vk::PipelineStageFlags2::empty(),
                },
                layout: vk::ImageLayout::UNDEFINED,
            },
            &mut old_usages,
        );

        let (dst_access, dst_stage) = (
            vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        match usage_hint {
            Some(usage) => match crate::util::texture_usage_to_stage_access(usage) {
                Some(res) => res,
                None => (
                    vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                    vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                ),
            },
            None => (
                vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            ),
        };

        for (i, old_usage) in old_usages.iter_mut().enumerate() {
            // Replace old usage
            let old_queue = std::mem::replace(
                &mut old_usage.queue,
                Some(QueueUsage {
                    queue: info.queue,
                    timeline_value: info.timeline_value,
                    command_idx,
                    is_async: info.is_async,
                }),
            );

            let (src_access, src_stage) =
                (old_usage.sub_resource.access, old_usage.sub_resource.stage);

            // If another command used this resource this command buffer, we add it as a dependency
            self.dependency_check(
                old_queue.as_ref(),
                command_idx,
                &mut info.wait_queues,
                (info.queue, info.timeline_value),
            );

            self.image_barriers.push(
                vk::ImageMemoryBarrier2::builder()
                    .src_queue_family_index(info.queue_families.to_index(info.queue))
                    .src_access_mask(src_access)
                    .src_stage_mask(src_stage)
                    .old_layout(old_usage.layout)
                    .dst_queue_family_index(info.queue_families.to_index(new_queue))
                    .dst_access_mask(dst_access)
                    .dst_stage_mask(dst_stage)
                    .new_layout(old_usage.layout)
                    .image(cube_map.internal().image)
                    .subresource_range(
                        vk::ImageSubresourceRange::builder()
                            .aspect_mask(cube_map.internal().aspect_flags)
                            .base_mip_level(base_mip + i as u32)
                            .level_count(1)
                            .base_array_layer(array_element)
                            .layer_count(1)
                            .build(),
                    )
                    .build(),
            );
        }
    }

    #[inline(always)]
    fn buffer_barrier_check(
        &mut self,
        queue_families: &QueueFamilyIndices,
        dst_queue_family_index: u32,
        old: &GlobalBufferUsage,
        new: &GlobalBufferUsage,
        buffer: vk::Buffer,
        sharing_mode: SharingMode,
        size: u64,
        offset: u64,
    ) -> bool {
        if old.same_command(new) {
            return false;
        }

        let barrier = match old.into_barrier(&new, sharing_mode) {
            Some(barrier) => barrier,
            None => return false,
        };

        match barrier {
            PipelineBarrier::Memory(barrier) => {
                self.memory_barriers.push(MemoryBarrier {
                    src_stage: barrier.src_stage_mask,
                    src_access: barrier.src_access_mask,
                    dst_stage: barrier.dst_stage_mask,
                    dst_access: barrier.dst_access_mask,
                });
            }
            PipelineBarrier::Buffer(mut barrier) => {
                barrier.buffer = buffer;

                let (mut src, mut dst) = match &old.queue {
                    Some(old) => (queue_families.to_index(old.queue), dst_queue_family_index),
                    None => (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED),
                };

                if src == dst {
                    src = vk::QUEUE_FAMILY_IGNORED;
                    dst = vk::QUEUE_FAMILY_IGNORED;
                }

                barrier.src_queue_family_index = src;
                barrier.dst_queue_family_index = dst;
                barrier.size = size;
                barrier.offset = offset;

                self.buffer_barriers.push(barrier);
            }
            PipelineBarrier::Image(_) => unreachable!(),
        }

        true
    }

    #[inline(always)]
    fn image_barrier_check(
        &mut self,
        queue_families: &QueueFamilyIndices,
        dst_queue_family_index: u32,
        old: &GlobalImageUsage,
        new: &GlobalImageUsage,
        image: vk::Image,
        aspect_flags: vk::ImageAspectFlags,
        array_element: u32,
        mip_level: u32,
    ) -> bool {
        if old.same_command(new) {
            return false;
        }

        let barrier = match old.into_barrier(&new) {
            Some(barrier) => barrier,
            None => return false,
        };

        match barrier {
            PipelineBarrier::Memory(barrier) => {
                self.memory_barriers.push(MemoryBarrier {
                    src_stage: barrier.src_stage_mask,
                    src_access: barrier.src_access_mask,
                    dst_stage: barrier.dst_stage_mask,
                    dst_access: barrier.dst_access_mask,
                });
            }
            PipelineBarrier::Image(mut barrier) => {
                barrier.image = image;
                barrier.subresource_range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(aspect_flags)
                    .base_mip_level(mip_level)
                    .level_count(1)
                    .base_array_layer(array_element)
                    .layer_count(1)
                    .build();

                match &old.queue {
                    Some(old) => {
                        barrier.src_queue_family_index = queue_families.to_index(old.queue);
                        barrier.dst_queue_family_index = dst_queue_family_index;
                    }
                    None => {
                        barrier.src_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                        barrier.dst_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                    }
                }

                self.image_barriers.push(barrier);
            }
            PipelineBarrier::Buffer(_) => unreachable!(),
        }

        true
    }

    #[inline(always)]
    fn dependency_check(
        &mut self,
        old_usage: Option<&QueueUsage>,
        command_idx: usize,
        wait_queues: &mut [Option<u64>; 4],
        cur_queue: (QueueType, u64),
    ) {
        if let Some(old_usage) = old_usage {
            // Register the old queue as needing to be waited on if the prevuous usage wasn't
            // async
            if !old_usage.is_async {
                let queue_value = match old_usage.queue {
                    QueueType::Main => &mut wait_queues[0],
                    QueueType::Transfer => &mut wait_queues[1],
                    QueueType::Compute => &mut wait_queues[2],
                    QueueType::Present => &mut wait_queues[3],
                };

                *queue_value = match *queue_value {
                    Some(old) => Some(old.max(old_usage.timeline_value)),
                    None => Some(old_usage.timeline_value),
                };
            }

            // Add depdency to the previous command
            if old_usage.queue == cur_queue.0
                && old_usage.timeline_value == cur_queue.1
                && old_usage.command_idx != command_idx
                && old_usage.command_idx != usize::MAX
            {
                self.commands[old_usage.command_idx]
                    .dependents
                    .push(command_idx);
                self.commands[command_idx].dependency_count += 1;
            }
        }
    }
}

impl From<MemoryBarrier> for vk::MemoryBarrier2 {
    #[inline(always)]
    fn from(value: MemoryBarrier) -> Self {
        vk::MemoryBarrier2::builder()
            .src_access_mask(value.src_access)
            .src_stage_mask(value.src_stage)
            .dst_access_mask(value.dst_access)
            .dst_stage_mask(value.dst_stage)
            .build()
    }
}
