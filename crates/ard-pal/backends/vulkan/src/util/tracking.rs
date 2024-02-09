use api::{
    buffer::Buffer,
    command_buffer::{
        BlitDestination, BlitSource, BufferCubeMapCopy, BufferTextureCopy, Command,
        CopyBufferToBuffer, TextureResolve,
    },
    cube_map::CubeMap,
    descriptor_set::DescriptorSet,
    render_pass::{ColorAttachmentSource, RenderPassDescriptor},
    texture::{Blit, Texture},
    types::{SharingMode, TextureUsage},
};

use super::{
    semaphores::{SemaphoreTracker, WaitInfo},
    usage::{PipelineTracker, SubResource, SubResourceUsage, UsageScope},
};
use ash::vk;

pub(crate) struct TrackState<'a, 'b> {
    pub device: &'a ash::Device,
    pub command_buffer: vk::CommandBuffer,
    /// Index of the command to detect the resources of.
    pub index: usize,
    /// Command list with all commands of a submit.
    pub commands: &'a [Command<'a, crate::VulkanBackend>],
    /// Used to detect inter-command dependencies.
    pub pipeline_tracker: &'a mut PipelineTracker<'b>,
    /// Used by `resc_state` to track inter-queue dependencies.
    pub semaphores: &'a mut SemaphoreTracker,
    /// Usage scope to reuse.
    pub scope: &'a mut UsageScope,
}

/// Given the index of a command in a command list, tracks resources based off the type of
/// detected command.
pub(crate) unsafe fn track_resources(mut state: TrackState) {
    puffin::profile_function!();

    state.scope.reset();

    match &state.commands[state.index] {
        Command::BeginRenderPass(descriptor) => track_render_pass(&mut state, descriptor),
        Command::Dispatch(_, _, _) => track_dispatch(&mut state),
        Command::CopyBufferToBuffer(copy_info) => {
            track_buffer_to_buffer_copy(&mut state, copy_info)
        }
        Command::CopyBufferToTexture {
            buffer,
            texture,
            copy,
        } => track_buffer_to_texture_copy(&mut state, buffer, texture, copy),
        Command::CopyTextureToBuffer {
            buffer,
            texture,
            copy,
        } => track_texture_to_buffer_copy(&mut state, buffer, texture, copy),
        Command::CopyBufferToCubeMap {
            buffer,
            cube_map,
            copy,
        } => track_buffer_to_cube_map_copy(&mut state, buffer, cube_map, copy),
        Command::CopyCubeMapToBuffer {
            cube_map,
            buffer,
            copy,
        } => track_cube_map_to_buffer_copy(&mut state, cube_map, buffer, copy),
        Command::Blit { src, dst, blit, .. } => track_blit(&mut state, src, dst, blit),
        Command::TextureResolve { src, dst, resolve } => {
            track_texture_resolve(&mut state, src, dst, resolve)
        }
        Command::SetTextureUsage {
            tex,
            new_usage,
            array_elem,
            base_mip,
            mip_count,
        } => track_set_texture_usage(
            &mut state,
            tex,
            *new_usage,
            *array_elem,
            *base_mip,
            *mip_count,
        ),
        // All other commands do not need state tracking
        _ => {}
    }
}

unsafe fn track_render_pass(
    state: &mut TrackState,
    descriptor: &RenderPassDescriptor<'_, crate::VulkanBackend>,
) {
    puffin::profile_function!();

    // Track color attachments used in the pass
    for attachment in &descriptor.color_attachments {
        let (subresource, layout) = match attachment.source {
            ColorAttachmentSource::SurfaceImage(image) => {
                // Surface image has special semaphores
                let semaphores = image.internal().semaphores();
                state
                    .semaphores
                    .register_signal(semaphores.presentable, None);
                state.semaphores.register_wait(
                    semaphores.available,
                    WaitInfo {
                        value: None,
                        stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    },
                );

                (
                    SubResource::Texture {
                        texture: image.internal().image(),
                        id: image.internal().id(),
                        sharing: SharingMode::Concurrent,
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        array_elem: 0,
                        base_mip_level: 0,
                        mip_count: 1,
                    },
                    vk::ImageLayout::PRESENT_SRC_KHR,
                )
            }
            ColorAttachmentSource::Texture {
                texture,
                array_element,
                mip_level,
            } => (
                SubResource::Texture {
                    texture: texture.internal().image,
                    id: texture.internal().id,
                    sharing: texture.sharing_mode(),
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    array_elem: array_element as u32,
                    base_mip_level: mip_level as u32,
                    mip_count: 1,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            ColorAttachmentSource::CubeMap {
                cube_map,
                face,
                array_element,
                mip_level,
                ..
            } => (
                SubResource::CubeFace {
                    cube_map: cube_map.internal().image,
                    face,
                    id: cube_map.internal().id,
                    sharing: cube_map.sharing_mode(),
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    array_elem: array_element as u32,
                    mip_level: mip_level as u32,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
        };
        state.scope.use_resource(
            &subresource,
            &SubResourceUsage {
                access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::COLOR_ATTACHMENT_READ,
                stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                layout,
            },
        );
    }

    for attachment in &descriptor.color_resolve_attachments {
        let (subresource, layout) = match attachment.dst {
            ColorAttachmentSource::SurfaceImage(image) => {
                // Surface image has special semaphores
                let semaphores = image.internal().semaphores();
                state
                    .semaphores
                    .register_signal(semaphores.presentable, None);
                state.semaphores.register_wait(
                    semaphores.available,
                    WaitInfo {
                        value: None,
                        stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    },
                );

                (
                    SubResource::Texture {
                        texture: image.internal().image(),
                        id: image.internal().id(),
                        sharing: SharingMode::Concurrent,
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        array_elem: 0,
                        base_mip_level: 0,
                        mip_count: 1,
                    },
                    vk::ImageLayout::PRESENT_SRC_KHR,
                )
            }
            ColorAttachmentSource::Texture {
                texture,
                array_element,
                mip_level,
            } => (
                SubResource::Texture {
                    texture: texture.internal().image,
                    id: texture.internal().id,
                    sharing: texture.sharing_mode(),
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    array_elem: array_element as u32,
                    base_mip_level: mip_level as u32,
                    mip_count: 1,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            ColorAttachmentSource::CubeMap {
                cube_map,
                face,
                array_element,
                mip_level,
                ..
            } => (
                SubResource::CubeFace {
                    cube_map: cube_map.internal().image,
                    face,
                    id: cube_map.internal().id,
                    sharing: cube_map.sharing_mode(),
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    array_elem: array_element as u32,
                    mip_level: mip_level as u32,
                },
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
        };
        state.scope.use_resource(
            &subresource,
            &SubResourceUsage {
                access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::COLOR_ATTACHMENT_READ,
                stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                layout,
            },
        );
    }

    // Track depth stencil attachment
    if let Some(attachment) = &descriptor.depth_stencil_attachment {
        let internal = attachment.texture.internal();
        state.scope.use_resource(
            &SubResource::Texture {
                texture: internal.image,
                id: internal.id,
                sharing: attachment.texture.sharing_mode(),
                aspect_mask: internal.aspect_flags,
                array_elem: attachment.array_element as u32,
                base_mip_level: attachment.mip_level as u32,
                mip_count: 1,
            },
            &SubResourceUsage {
                access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            },
        );
    }

    if let Some(attachment) = &descriptor.depth_stencil_resolve_attachment {
        let internal = attachment.dst.internal();
        state.scope.use_resource(
            &SubResource::Texture {
                texture: internal.image,
                id: internal.id,
                sharing: attachment.dst.sharing_mode(),
                aspect_mask: internal.aspect_flags,
                array_elem: attachment.array_element as u32,
                base_mip_level: attachment.mip_level as u32,
                mip_count: 1,
            },
            &SubResourceUsage {
                access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            },
        );
    }

    // Track everything else
    for command in &state.commands[state.index..] {
        match command {
            Command::BindVertexBuffers { binds, .. } => {
                for bind in binds {
                    state.scope.use_resource(
                        &SubResource::Buffer {
                            buffer: bind.buffer.internal().buffer,
                            id: bind.buffer.internal().id,
                            sharing: bind.buffer.sharing_mode(),
                            aligned_size: bind.buffer.internal().aligned_size as usize,
                            array_elem: bind.array_element as u32,
                        },
                        &SubResourceUsage {
                            access: vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
                            stage: vk::PipelineStageFlags::VERTEX_INPUT,
                            layout: vk::ImageLayout::UNDEFINED,
                        },
                    );
                }
            }
            Command::BindIndexBuffer {
                buffer,
                array_element,
                ..
            } => {
                state.scope.use_resource(
                    &SubResource::Buffer {
                        buffer: buffer.internal().buffer,
                        id: buffer.internal().id,
                        sharing: buffer.sharing_mode(),
                        aligned_size: buffer.internal().aligned_size as usize,
                        array_elem: *array_element as u32,
                    },
                    &SubResourceUsage {
                        access: vk::AccessFlags::INDEX_READ,
                        stage: vk::PipelineStageFlags::VERTEX_INPUT,
                        layout: vk::ImageLayout::UNDEFINED,
                    },
                );
            }
            Command::BindDescriptorSets { sets, .. } => {
                track_descriptor_sets(
                    sets,
                    vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    state,
                );
            }
            Command::DrawIndexedIndirect {
                buffer,
                array_element,
                ..
            } => {
                state.scope.use_resource(
                    &SubResource::Buffer {
                        buffer: buffer.internal().buffer,
                        id: buffer.internal().id,
                        sharing: buffer.sharing_mode(),
                        aligned_size: buffer.internal().aligned_size as usize,
                        array_elem: *array_element as u32,
                    },
                    &SubResourceUsage {
                        access: vk::AccessFlags::INDIRECT_COMMAND_READ,
                        stage: vk::PipelineStageFlags::DRAW_INDIRECT,
                        layout: vk::ImageLayout::UNDEFINED,
                    },
                );
            }
            Command::DrawIndexedIndirectCount {
                draw_buffer,
                draw_array_element,
                count_buffer,
                count_array_element,
                ..
            } => {
                state.scope.use_resource(
                    &SubResource::Buffer {
                        buffer: draw_buffer.internal().buffer,
                        id: draw_buffer.internal().id,
                        sharing: draw_buffer.sharing_mode(),
                        aligned_size: draw_buffer.internal().aligned_size as usize,
                        array_elem: *draw_array_element as u32,
                    },
                    &SubResourceUsage {
                        access: vk::AccessFlags::INDIRECT_COMMAND_READ,
                        stage: vk::PipelineStageFlags::DRAW_INDIRECT,
                        layout: vk::ImageLayout::UNDEFINED,
                    },
                );

                state.scope.use_resource(
                    &SubResource::Buffer {
                        buffer: count_buffer.internal().buffer,
                        id: count_buffer.internal().id,
                        sharing: count_buffer.sharing_mode(),
                        aligned_size: count_buffer.internal().aligned_size as usize,
                        array_elem: *count_array_element as u32,
                    },
                    &SubResourceUsage {
                        access: vk::AccessFlags::INDIRECT_COMMAND_READ,
                        stage: vk::PipelineStageFlags::DRAW_INDIRECT,
                        layout: vk::ImageLayout::UNDEFINED,
                    },
                );
            }
            Command::EndRenderPass => break,
            _ => {}
        }
    }

    // Submit usage scope
    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_dispatch(state: &mut TrackState) {
    puffin::profile_function!();

    // Find the index of the bound pipeline
    let idx = {
        let mut idx = None;
        for (i, command) in state.commands[..=state.index].iter().enumerate().rev() {
            match command {
                Command::BindComputePipeline(_) => {
                    idx = Some(i);
                    break;
                }
                Command::BeginComputePass => break,
                _ => {}
            }
        }

        match idx {
            Some(idx) => idx,
            // No bound pipeline so no state track needed
            None => return,
        }
    };

    // Determine how many sets are used by the active pipeline
    let mut total_bound = 0;
    let mut bound = {
        let pipeline = match &state.commands[idx] {
            Command::BindComputePipeline(pipeline) => pipeline,
            // Unreachable because of early return in previous pass
            _ => unreachable!(),
        };
        let mut bound = Vec::with_capacity(pipeline.layouts().len());
        bound.resize(pipeline.layouts().len(), false);
        bound
    };

    // Determine which sets are actually used
    for command in state.commands[idx..=state.index].iter().rev() {
        // Break early if every set is bound
        if total_bound == bound.len() {
            break;
        }

        // Grab bind info. Skip other commands
        let (sets, first) = match command {
            Command::BindDescriptorSets { sets, first, .. } => (sets, *first),
            _ => continue,
        };

        // Track sets
        for (i, set_slot) in (first..(first + sets.len())).enumerate() {
            // Skip if the set slot is already bound
            if bound[set_slot] {
                continue;
            }

            // Track
            track_descriptor_set(sets[i], vk::PipelineStageFlags::COMPUTE_SHADER, state);
            bound[set_slot] = true;
            total_bound += 1;
        }
    }

    // Submit pipeline values
    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_buffer_to_buffer_copy(
    state: &mut TrackState,
    copy: &CopyBufferToBuffer<'_, crate::VulkanBackend>,
) {
    puffin::profile_function!();

    // Barrier check
    let src = copy.src.internal();
    let dst = copy.dst.internal();

    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: src.buffer,
            id: src.id,
            sharing: copy.src.sharing_mode(),
            array_elem: copy.src_array_element as u32,
            aligned_size: copy.src.internal().aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );
    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: dst.buffer,
            id: dst.id,
            sharing: copy.dst.sharing_mode(),
            array_elem: copy.dst_array_element as u32,
            aligned_size: copy.dst.internal().aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_buffer_to_texture_copy(
    state: &mut TrackState,
    buffer: &Buffer<crate::VulkanBackend>,
    texture: &Texture<crate::VulkanBackend>,
    copy: &BufferTextureCopy,
) {
    puffin::profile_function!();

    // Barrier check
    let buffer_int = buffer.internal();
    let texture_int = texture.internal();

    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: buffer_int.buffer,
            id: buffer_int.id,
            sharing: buffer.sharing_mode(),
            array_elem: copy.buffer_array_element as u32,
            aligned_size: buffer.internal().aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );
    state.scope.use_resource(
        &SubResource::Texture {
            texture: texture_int.image,
            id: texture_int.id,
            sharing: texture.sharing_mode(),
            aspect_mask: texture_int.aspect_flags,
            array_elem: copy.texture_array_element as u32,
            base_mip_level: copy.texture_mip_level as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_texture_to_buffer_copy(
    state: &mut TrackState,
    buffer: &Buffer<crate::VulkanBackend>,
    texture: &Texture<crate::VulkanBackend>,
    copy: &BufferTextureCopy,
) {
    puffin::profile_function!();

    // Barrier check
    let buffer_int = buffer.internal();
    let texture_int = texture.internal();

    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: buffer_int.buffer,
            id: buffer_int.id,
            sharing: buffer.sharing_mode(),
            array_elem: copy.buffer_array_element as u32,
            aligned_size: buffer.internal().aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );
    state.scope.use_resource(
        &SubResource::Texture {
            texture: texture_int.image,
            id: texture_int.id,
            sharing: texture.sharing_mode(),
            aspect_mask: texture_int.aspect_flags,
            array_elem: copy.texture_array_element as u32,
            base_mip_level: copy.texture_mip_level as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_buffer_to_cube_map_copy(
    state: &mut TrackState,
    buffer: &Buffer<crate::VulkanBackend>,
    cube_map: &CubeMap<crate::VulkanBackend>,
    copy: &BufferCubeMapCopy,
) {
    puffin::profile_function!();

    // Barrier check
    let buffer_int = buffer.internal();
    let cube_map_int = cube_map.internal();

    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: buffer_int.buffer,
            id: buffer_int.id,
            sharing: buffer.sharing_mode(),
            array_elem: copy.buffer_array_element as u32,
            aligned_size: buffer_int.aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );
    state.scope.use_resource(
        &SubResource::CubeMap {
            cube_map: cube_map_int.image,
            id: cube_map_int.id,
            sharing: cube_map.sharing_mode(),
            aspect_mask: cube_map_int.aspect_flags,
            array_elem: copy.cube_map_array_element as u32,
            mip_level: copy.cube_map_mip_level as u32,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_cube_map_to_buffer_copy(
    state: &mut TrackState,
    cube_map: &CubeMap<crate::VulkanBackend>,
    buffer: &Buffer<crate::VulkanBackend>,
    copy: &BufferCubeMapCopy,
) {
    puffin::profile_function!();

    // Barrier check
    let buffer_int = buffer.internal();
    let cube_map_int = cube_map.internal();

    state.scope.use_resource(
        &SubResource::Buffer {
            buffer: buffer_int.buffer,
            id: buffer_int.id,
            sharing: buffer.sharing_mode(),
            array_elem: copy.buffer_array_element as u32,
            aligned_size: buffer_int.aligned_size as usize,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );
    state.scope.use_resource(
        &SubResource::CubeMap {
            cube_map: cube_map_int.image,
            id: cube_map_int.id,
            sharing: cube_map.sharing_mode(),
            aspect_mask: cube_map_int.aspect_flags,
            array_elem: copy.cube_map_array_element as u32,
            mip_level: copy.cube_map_mip_level as u32,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_blit(
    state: &mut TrackState,
    src: &BlitSource<crate::VulkanBackend>,
    dst: &BlitDestination<crate::VulkanBackend>,
    blit: &Blit,
) {
    puffin::profile_function!();

    // Barrier check
    let (src_img, src_id, src_array_elem, src_aspect_flags, src_sharing_mode) = match src {
        BlitSource::Texture(tex) => {
            let internal = tex.internal();
            (
                internal.image,
                internal.id,
                blit.src_array_element,
                internal.aspect_flags,
                tex.sharing_mode(),
            )
        }
        BlitSource::CubeMap { cube_map, face } => {
            let internal = cube_map.internal();
            (
                internal.image,
                internal.id,
                crate::cube_map::CubeMap::to_array_elem(blit.src_array_element, *face),
                internal.aspect_flags,
                cube_map.sharing_mode(),
            )
        }
    };

    let (dst_img, dst_id, dst_array_elem, dst_aspect_flags, dst_sharing_mode) = match dst {
        BlitDestination::Texture(tex) => {
            let internal = tex.internal();
            (
                internal.image,
                internal.id,
                blit.dst_array_element,
                internal.aspect_flags,
                tex.sharing_mode(),
            )
        }
        BlitDestination::CubeMap { cube_map, face } => {
            let internal = cube_map.internal();
            (
                internal.image,
                internal.id,
                crate::cube_map::CubeMap::to_array_elem(blit.dst_array_element, *face),
                internal.aspect_flags,
                cube_map.sharing_mode(),
            )
        }
        BlitDestination::SurfaceImage(si) => {
            let internal = si.internal();
            let semaphores = internal.semaphores();

            // Also handle semaphores of the surface image
            state
                .semaphores
                .register_signal(semaphores.presentable, None);
            state.semaphores.register_wait(
                semaphores.available,
                WaitInfo {
                    value: None,
                    stage: vk::PipelineStageFlags::TRANSFER,
                },
            );

            (
                internal.image(),
                internal.id(),
                0,
                vk::ImageAspectFlags::COLOR,
                SharingMode::Concurrent,
            )
        }
    };

    state.scope.use_resource(
        &SubResource::Texture {
            texture: src_img,
            id: src_id,
            sharing: src_sharing_mode,
            aspect_mask: src_aspect_flags,
            array_elem: src_array_elem as u32,
            base_mip_level: blit.src_mip as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        },
    );
    state.scope.use_resource(
        &SubResource::Texture {
            texture: dst_img,
            id: dst_id,
            sharing: dst_sharing_mode,
            aspect_mask: dst_aspect_flags,
            array_elem: dst_array_elem as u32,
            base_mip_level: blit.dst_mip as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_texture_resolve(
    state: &mut TrackState,
    src: &Texture<crate::VulkanBackend>,
    dst: &Texture<crate::VulkanBackend>,
    resolve: &TextureResolve,
) {
    puffin::profile_function!();

    state.scope.use_resource(
        &SubResource::Texture {
            texture: src.internal().image,
            id: src.internal().id,
            sharing: src.sharing_mode(),
            aspect_mask: src.internal().aspect_flags,
            array_elem: resolve.src_array_element as u32,
            base_mip_level: resolve.src_mip as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        },
    );
    state.scope.use_resource(
        &SubResource::Texture {
            texture: dst.internal().image,
            id: dst.internal().id,
            sharing: dst.sharing_mode(),
            aspect_mask: dst.internal().aspect_flags,
            array_elem: resolve.dst_array_element as u32,
            base_mip_level: resolve.dst_mip as u32,
            mip_count: 1,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}

unsafe fn track_descriptor_sets(
    sets: &[&DescriptorSet<crate::VulkanBackend>],
    set_stage: vk::PipelineStageFlags,
    state: &mut TrackState,
) {
    puffin::profile_function!();

    for set in sets {
        track_descriptor_set(set, set_stage, state);
    }
}

unsafe fn track_descriptor_set(
    set: &DescriptorSet<crate::VulkanBackend>,
    set_stage: vk::PipelineStageFlags,
    state: &mut TrackState,
) {
    puffin::profile_function!();

    state.scope.use_resource(
        &SubResource::Set {
            id: set.internal().id,
        },
        &SubResourceUsage {
            access: vk::AccessFlags::empty(),
            stage: set_stage,
            layout: vk::ImageLayout::UNDEFINED,
        },
    );

    // Check every binding of every set
    for (subresource, usage) in &set.internal().resource_usage {
        state.scope.use_resource(subresource, usage);
    }
}

unsafe fn track_set_texture_usage(
    state: &mut TrackState,
    tex: &Texture<crate::VulkanBackend>,
    new_usage: TextureUsage,
    array_elem: usize,
    base_mip: u32,
    mip_count: usize,
) {
    puffin::profile_function!();

    state.scope.use_resource(
        &SubResource::Texture {
            texture: tex.internal().image,
            id: tex.internal().id,
            aspect_mask: tex.internal().aspect_flags,
            array_elem: array_elem as u32,
            base_mip_level: base_mip,
            mip_count: mip_count as u32,
            sharing: tex.sharing_mode(),
        },
        &SubResourceUsage {
            access: vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE,
            stage: vk::PipelineStageFlags::TRANSFER,
            layout: match new_usage {
                TextureUsage::COLOR_ATTACHMENT => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                TextureUsage::DEPTH_STENCIL_ATTACHMENT => {
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                }
                TextureUsage::TRANSFER_DST => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                TextureUsage::TRANSFER_SRC => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                TextureUsage::STORAGE => vk::ImageLayout::GENERAL,
                TextureUsage::SAMPLED => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                _ => unreachable!("command guarantees only one usage"),
            },
        },
    );

    if let Some(barrier) = state.pipeline_tracker.submit(state.scope) {
        barrier.execute(state.device, state.command_buffer);
    }
}
