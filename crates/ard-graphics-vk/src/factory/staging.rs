use ard_graphics_api::{mesh::VertexLayout, prelude::MipType};
use ash::vk;
use glam::{Vec2, Vec4};
use std::{
    collections::HashMap,
    ops::{Div, Shr},
    sync::Arc,
};

use crate::{
    alloc::{Buffer, Image},
    context::GraphicsContext,
};

use super::meshes::{Block, MeshBuffers};

pub(crate) enum StagingRequest {
    Mesh {
        id: u32,
        layout: VertexLayout,
        vertex_count: usize,
        vertex_staging: Buffer,
        index_staging: Buffer,
        vertex_dst: Block,
        index_dst: Block,
    },
    Texture {
        id: u32,
        image_dst: Arc<Image>,
        staging_buffer: Buffer,
        mip_type: MipType,
    },
    TextureMipUpload {
        id: u32,
        image_dst: Arc<Image>,
        staging_buffer: Buffer,
        mip_level: u32,
    },
}

pub(crate) struct StagingBuffers {
    ctx: GraphicsContext,
    pool: vk::CommandPool,
    uploads: Vec<Upload>,
    free_commands: Vec<(vk::CommandBuffer, vk::Fence)>,
    /// Holds staging requests until they are complete so buffers are dropped appropriately.
    holding_pen: HashMap<ResourceId, StagingRequest>,
    pending_requests: Vec<StagingRequest>,
}

struct Upload {
    /// Command buffer that was used to upload the staging buffers.
    command: vk::CommandBuffer,
    /// Fence to check to see if the upload is complete.
    fence: vk::Fence,
    /// List of resources that were uploaded.
    resources: Vec<ResourceId>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ResourceId {
    Mesh(u32),
    Texture(u32),
    TextureMip { texture_id: u32, mip_level: u32 },
}

impl StagingBuffers {
    pub unsafe fn new(ctx: &GraphicsContext) -> Self {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(ctx.0.queue_family_indices.transfer)
            .build();

        let pool = ctx
            .0
            .device
            .create_command_pool(&create_info, None)
            .expect("unable to create staging buffer command pool");

        StagingBuffers {
            ctx: ctx.clone(),
            pool,
            uploads: Vec::default(),
            free_commands: Vec::default(),
            pending_requests: Vec::default(),
            holding_pen: HashMap::default(),
        }
    }

    pub fn add(&mut self, request: StagingRequest) {
        self.pending_requests.push(request);
    }

    /// Checks if any uploads are complete. Runs a closure for each resource that is complete.
    pub unsafe fn flush_complete_uploads(
        &mut self,
        blocking: bool,
        on_complete: &mut impl FnMut(ResourceId),
    ) {
        // TODO: When drain filter gets put into stable, this can all be done in one function chain

        let mut to_remove = Vec::default();

        for (i, upload) in self.uploads.iter_mut().enumerate() {
            // Check to see if the command is finished
            let fence = [upload.fence];
            let uploaded = match self.ctx.0.device.wait_for_fences(
                &fence,
                true,
                if blocking { u64::MAX } else { 0 },
            ) {
                Ok(()) => true,
                Err(err) => match err {
                    vk::Result::TIMEOUT => false,
                    err => panic!("error waiting on staging fence: {}", err),
                },
            };

            if uploaded {
                to_remove.push(i);
                self.ctx
                    .0
                    .device
                    .reset_fences(&fence)
                    .expect("unable to reset staging fence");
                self.free_commands.push((upload.command, upload.fence));

                // Destroy holding pen resources and run closure
                for resource in &upload.resources {
                    self.holding_pen.remove(resource);
                    on_complete(*resource);
                }
            }
        }

        // Removes finished commands
        to_remove.sort_unstable();
        for i in to_remove.into_iter().rev() {
            self.uploads.swap_remove(i);
        }
    }

    /// Begin pending uploads.
    pub unsafe fn upload(&mut self, mesh_buffers: &mut MeshBuffers) {
        let device = &self.ctx.0.device;

        if self.pending_requests.is_empty() {
            return;
        }

        // Either grab a free command buffer and fence or allocate a new one
        let (commands, fence) = if let Some(out) = self.free_commands.pop() {
            out
        } else {
            let create_info = vk::FenceCreateInfo::builder().build();

            let fence = device
                .create_fence(&create_info, None)
                .expect("unable to create staging fence");

            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(self.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            let commands = device
                .allocate_command_buffers(&alloc_info)
                .expect("unable to allocate staging commands")[0];

            (commands, fence)
        };

        let mut upload = Upload {
            command: commands,
            fence,
            resources: Vec::default(),
        };

        // Write transfer operations
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();

        device
            .begin_command_buffer(commands, &begin_info)
            .expect("unable to begin staging commands");

        for mut request in self.pending_requests.drain(..) {
            let id = match &mut request {
                StagingRequest::Mesh {
                    id,
                    layout,
                    vertex_count,
                    vertex_staging,
                    index_staging,
                    vertex_dst,
                    index_dst,
                } => {
                    // Write index data
                    let ib = mesh_buffers.get_index_buffer();
                    let index_regions = [vk::BufferCopy::builder()
                        .dst_offset(index_dst.base() as u64 * std::mem::size_of::<u32>() as u64)
                        .size(index_staging.size)
                        .build()];

                    device.cmd_copy_buffer(
                        commands,
                        index_staging.buffer(),
                        ib.buffer(),
                        &index_regions,
                    );

                    // Write vertex data
                    let mut cur_buffer = 0;
                    let mut cur_vertex_offset = 0;
                    let vbs = mesh_buffers.get_vertex_buffer(layout);

                    let mut copy_to_buffer = |elem_size: usize| {
                        let copy_len = (*vertex_count * elem_size) as u64;
                        let region = [vk::BufferCopy::builder()
                            .src_offset(cur_vertex_offset)
                            .dst_offset(vertex_dst.base() as u64 * elem_size as u64)
                            .size(copy_len)
                            .build()];

                        device.cmd_copy_buffer(
                            commands,
                            vertex_staging.buffer(),
                            vbs.buffer(cur_buffer),
                            &region,
                        );
                        cur_vertex_offset += copy_len;
                        cur_buffer += 1;
                    };

                    // Position
                    copy_to_buffer(std::mem::size_of::<Vec4>());

                    // Normals
                    if layout.normals {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }

                    // Tangents
                    if layout.tangents {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }

                    // Colors
                    if layout.colors {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }

                    // UV0s
                    if layout.uv0 {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }

                    // UV1s
                    if layout.uv1 {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }

                    // UV2s
                    if layout.uv2 {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }

                    // UV3s
                    if layout.uv3 {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }

                    ResourceId::Mesh(*id)
                }
                StagingRequest::Texture {
                    id,
                    image_dst,
                    staging_buffer,
                    mip_type,
                } => match mip_type {
                    // Mip levels must be generated from LOD0 contained in the staging buffer.
                    MipType::Generate => {
                        // Copy image to LOD0 mip
                        transition_image_layout(
                            device,
                            commands,
                            image_dst,
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            0,
                            image_dst.mip_levels(),
                        );

                        buffer_to_image_copy(device, commands, image_dst, staging_buffer, 0);

                        // Copy down the LOD chain
                        let mut mip_width = image_dst.width();
                        let mut mip_height = image_dst.height();

                        for i in 1..image_dst.mip_levels() {
                            // Transition previous LOD to be a transfer source
                            transition_image_layout(
                                device,
                                commands,
                                image_dst,
                                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                                i,
                                1,
                            );

                            // Blit from previous LOD to current LOD
                            let blit = [vk::ImageBlit::builder()
                                .src_offsets([
                                    vk::Offset3D::default(),
                                    vk::Offset3D {
                                        x: mip_width as i32,
                                        y: mip_height as i32,
                                        z: 1,
                                    },
                                ])
                                .src_subresource(
                                    vk::ImageSubresourceLayers::builder()
                                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                                        .mip_level(i - 1)
                                        .base_array_layer(0)
                                        .layer_count(1)
                                        .build(),
                                )
                                .dst_offsets([
                                    vk::Offset3D::default(),
                                    vk::Offset3D {
                                        x: mip_width.div(2).max(1) as i32,
                                        y: mip_height.div(2).max(1) as i32,
                                        z: 1,
                                    },
                                ])
                                .dst_subresource(
                                    vk::ImageSubresourceLayers::builder()
                                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                                        .mip_level(i)
                                        .base_array_layer(0)
                                        .layer_count(1)
                                        .build(),
                                )
                                .build()];

                            device.cmd_blit_image(
                                commands,
                                image_dst.image(),
                                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                                image_dst.image(),
                                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                                &blit,
                                vk::Filter::LINEAR,
                            );

                            mip_width = (mip_width / 2).max(1);
                            mip_height = (mip_height / 2).max(1);
                        }

                        // Final transition
                        transition_image_layout(
                            device,
                            commands,
                            image_dst,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            image_dst.mip_levels() - 1,
                            1,
                        );

                        transition_image_layout(
                            device,
                            commands,
                            image_dst,
                            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            0,
                            image_dst.mip_levels() - 1,
                        );

                        ResourceId::Texture(*id)
                    }
                    // Only the highest level LOD is contained in the staging buffer.
                    MipType::Upload => {
                        transition_image_layout(
                            device,
                            commands,
                            image_dst,
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            0,
                            image_dst.mip_levels(),
                        );

                        buffer_to_image_copy(
                            device,
                            commands,
                            image_dst,
                            staging_buffer,
                            image_dst.mip_levels().saturating_sub(1),
                        );

                        transition_image_layout(
                            device,
                            commands,
                            image_dst,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            0,
                            image_dst.mip_levels(),
                        );

                        ResourceId::Texture(*id)
                    }
                },
                StagingRequest::TextureMipUpload {
                    id,
                    image_dst,
                    staging_buffer,
                    mip_level,
                } => {
                    transition_image_layout(
                        device,
                        commands,
                        image_dst,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        *mip_level,
                        1,
                    );

                    buffer_to_image_copy(device, commands, image_dst, staging_buffer, *mip_level);

                    transition_image_layout(
                        device,
                        commands,
                        image_dst,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        *mip_level,
                        1,
                    );

                    ResourceId::TextureMip {
                        mip_level: *mip_level,
                        texture_id: *id,
                    }
                }
            };

            upload.resources.push(id);
            self.holding_pen.insert(id, request);
        }

        device
            .end_command_buffer(commands)
            .expect("unable to end staging commands");

        // Submit transfer
        let submit_commands = [commands];

        let submit = [vk::SubmitInfo::builder()
            .command_buffers(&submit_commands)
            .build()];

        device
            .queue_submit(self.ctx.0.transfer, &submit, fence)
            .expect("unable to submit transfer commands to staging queue");

        // Submit to uploads
        self.uploads.push(upload);
    }
}

impl Drop for StagingBuffers {
    fn drop(&mut self) {
        unsafe {
            self.flush_complete_uploads(true, &mut |_| {});

            for (_, fence) in self.free_commands.drain(..) {
                self.ctx.0.device.destroy_fence(fence, None);
            }

            self.ctx.0.device.destroy_command_pool(self.pool, None);
        }
    }
}

pub(crate) unsafe fn buffer_to_image_copy(
    device: &ash::Device,
    commands: vk::CommandBuffer,
    image: &Image,
    buffer: &Buffer,
    mip_level: u32,
) {
    let regions = [vk::BufferImageCopy::builder()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width: image.width().shr(mip_level).max(1),
            height: image.height().shr(mip_level).max(1),
            depth: 1,
        })
        .build()];

    device.cmd_copy_buffer_to_image(
        commands,
        buffer.buffer(),
        image.image(),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &regions,
    );
}

pub(crate) unsafe fn transition_image_layout(
    device: &ash::Device,
    commands: vk::CommandBuffer,
    image: &Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    base_mip: u32,
    mip_count: u32,
) {
    if mip_count == 0 {
        return;
    }

    let mut barrier = vk::ImageMemoryBarrier::builder()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image.image())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: base_mip,
            level_count: mip_count,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build();

    let src_stage;
    let dst_stage;

    // Initial transition
    if old_layout == vk::ImageLayout::UNDEFINED
        && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::empty();
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        src_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
        dst_stage = vk::PipelineStageFlags::TRANSFER;
    }
    // Transition for image to buffer copy
    else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::empty();

        src_stage = vk::PipelineStageFlags::TRANSFER;
        dst_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    }
    // Transition for mip generation completion
    else if old_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
        && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        barrier.dst_access_mask = vk::AccessFlags::empty();

        src_stage = vk::PipelineStageFlags::TRANSFER;
        dst_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    }
    // Transition for mip generation
    else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        && new_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

        src_stage = vk::PipelineStageFlags::TRANSFER;
        dst_stage = vk::PipelineStageFlags::TRANSFER;
    } else {
        panic!("unsupported transition");
    }

    let barrier = [barrier];

    device.cmd_pipeline_barrier(
        commands,
        src_stage,
        dst_stage,
        vk::DependencyFlags::BY_REGION,
        &[],
        &[],
        &barrier,
    );
}
