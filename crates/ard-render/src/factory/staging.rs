use std::ops::{Div, Shr};

use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;

use crate::{
    mesh::VertexLayout,
    texture::{MipType, TextureInner},
};

use super::{
    allocator::{ResourceAllocator, ResourceId},
    meshes::{MeshBlock, MeshBuffers},
};

pub(crate) enum StagingRequest {
    Mesh {
        id: ResourceId,
        layout: VertexLayout,
        vertex_count: usize,
        vertex_staging: Buffer,
        index_staging: Buffer,
        vertex_dst: MeshBlock,
        index_dst: MeshBlock,
    },
    Texture {
        id: ResourceId,
        staging_buffer: Buffer,
        mip_type: MipType,
    },
}

pub(crate) struct Staging {
    ctx: Context,
    uploads: Vec<Upload>,
    pending: Vec<StagingRequest>,
}

struct Upload {
    transfer_job: Job,
    main_job: Option<Job>,
    resources: Vec<StagingResource>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StagingResource {
    Mesh(ResourceId),
    Texture(ResourceId),
}

impl Staging {
    pub fn new(ctx: Context) -> Self {
        Staging {
            ctx,
            uploads: Vec::default(),
            pending: Vec::default(),
        }
    }

    #[inline(always)]
    pub fn add(&mut self, request: StagingRequest) {
        self.pending.push(request);
    }

    /// Checks if any uploads are complete. Runs a closure for each resource that is complete.
    pub fn flush_complete_uploads(
        &mut self,
        blocking: bool,
        mut on_complete: impl FnMut(StagingResource),
    ) {
        // TODO: When drain filter gets put into stable, this can all be done in one function chain
        let mut to_remove = Vec::default();
        for (i, upload) in self.uploads.iter_mut().enumerate() {
            if let Some(main_job) = &upload.main_job {
                if main_job.poll_status() == JobStatus::Running {
                    continue;
                }
            }
            if upload.transfer_job.poll_status() == JobStatus::Complete {
                to_remove.push(i);
                for resource in &upload.resources {
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
    pub fn upload(
        &mut self,
        mesh_buffers: &mut MeshBuffers,
        textures: &mut ResourceAllocator<TextureInner>,
    ) {
        if self.pending.is_empty() {
            return;
        }

        let mut resources = Vec::default();
        let mut transfer_commands = self.ctx.transfer().command_buffer();
        let mut main_commands = None;

        for request in &self.pending {
            let resc = match request {
                StagingRequest::Mesh {
                    id,
                    layout,
                    vertex_count,
                    vertex_staging,
                    index_staging,
                    vertex_dst,
                    index_dst,
                } => {
                    // Copy index data
                    let ib = mesh_buffers.get_index_buffer();
                    transfer_commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                        src: index_staging,
                        src_array_element: 0,
                        src_offset: 0,
                        dst: ib.buffer(),
                        dst_array_element: 0,
                        dst_offset: index_dst.base() as u64 * std::mem::size_of::<u32>() as u64,
                        len: index_staging.size(),
                    });

                    // Copy vertex data
                    let mut cur_buffer = 0;
                    let mut cur_vertex_offset = 0;
                    let vbs = mesh_buffers.get_vertex_buffer(*layout).unwrap();

                    let mut copy_to_buffer = |elem_size: usize| {
                        let copy_len = (*vertex_count * elem_size) as u64;
                        transfer_commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                            src: vertex_staging,
                            src_array_element: 0,
                            src_offset: cur_vertex_offset,
                            dst: vbs.buffer(cur_buffer),
                            dst_array_element: 0,
                            dst_offset: vertex_dst.base() as u64 * elem_size as u64,
                            len: copy_len,
                        });
                        cur_vertex_offset += copy_len;
                        cur_buffer += 1;
                    };

                    // Position
                    copy_to_buffer(std::mem::size_of::<Vec4>());

                    if layout.contains(VertexLayout::NORMAL) {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }
                    if layout.contains(VertexLayout::TANGENT) {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }
                    if layout.contains(VertexLayout::COLOR) {
                        copy_to_buffer(std::mem::size_of::<Vec4>());
                    }
                    if layout.contains(VertexLayout::UV0) {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }
                    if layout.contains(VertexLayout::UV1) {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }
                    if layout.contains(VertexLayout::UV2) {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }
                    if layout.contains(VertexLayout::UV3) {
                        copy_to_buffer(std::mem::size_of::<Vec2>());
                    }

                    StagingResource::Mesh(*id)
                }
                StagingRequest::Texture {
                    id,
                    staging_buffer,
                    mip_type,
                } => match *mip_type {
                    MipType::Generate => {
                        let dst = match textures.get(*id) {
                            Some(tex) => tex,
                            // Texture was dropped so upload is no longer needed
                            None => continue,
                        };

                        let commands = match main_commands.as_mut() {
                            Some(commands) => commands,
                            None => {
                                let new_commands = self.ctx.main().command_buffer();
                                main_commands = Some(new_commands);
                                main_commands.as_mut().unwrap()
                            }
                        };

                        // Copy lowest level mip from staging
                        commands.copy_buffer_to_texture(
                            &dst.texture,
                            staging_buffer,
                            BufferTextureCopy {
                                buffer_offset: 0,
                                buffer_row_length: 0,
                                buffer_image_height: 0,
                                buffer_array_element: 0,
                                texture_offset: (0, 0, 0),
                                texture_extent: dst.texture.dims(),
                                texture_mip_level: 0,
                                texture_array_element: 0,
                            },
                        );

                        // Blit each image in the LOD chain
                        let (mut mip_width, mut mip_height, _) = dst.texture.dims();
                        for i in 1..(dst.mip_levels as usize) {
                            commands.blit_texture(
                                &dst.texture,
                                BlitDestination::Texture(&dst.texture),
                                Blit {
                                    src_min: (0, 0, 0),
                                    src_max: (mip_width, mip_height, 1),
                                    src_mip: i - 1,
                                    src_array_element: 0,
                                    dst_min: (0, 0, 0),
                                    dst_max: (mip_width.div(2).max(1), mip_height.div(2).max(1), 1),
                                    dst_mip: i,
                                    dst_array_element: 0,
                                },
                                Filter::Linear,
                            );
                            mip_width = mip_width.div(2).max(1);
                            mip_height = mip_height.div(2).max(1);
                        }

                        StagingResource::Texture(*id)
                    }
                    MipType::Upload => {
                        let dst = match textures.get(*id) {
                            Some(tex) => tex,
                            // Texture was dropped so upload is no longer needed
                            None => continue,
                        };

                        let mip_level = dst.mip_levels.saturating_sub(1) as usize;
                        let (mut width, mut height, _) = dst.texture.dims();
                        width = width.shr(mip_level).max(1);
                        height = height.shr(mip_level).max(1);

                        transfer_commands.copy_buffer_to_texture(
                            &dst.texture,
                            staging_buffer,
                            BufferTextureCopy {
                                buffer_offset: 0,
                                buffer_row_length: 0,
                                buffer_image_height: 0,
                                buffer_array_element: 0,
                                texture_offset: (0, 0, 0),
                                texture_extent: (width, height, 1),
                                texture_mip_level: mip_level,
                                texture_array_element: 0,
                            },
                        );

                        StagingResource::Texture(*id)
                    }
                },
            };
            resources.push(resc);
        }

        // Submit jobs
        let transfer_job = self
            .ctx
            .transfer()
            .submit(Some("transfer_staging"), transfer_commands);
        let main_job =
            main_commands.map(|commands| self.ctx.main().submit(Some("main_staging"), commands));

        // Clear staging requests now that they are processed
        self.pending.clear();

        self.uploads.push(Upload {
            transfer_job,
            main_job,
            resources,
        });
    }
}
