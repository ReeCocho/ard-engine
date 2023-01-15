use std::ops::{Div, Shr};

use ard_formats::mesh::VertexLayout;
use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;

use crate::{
    cube_map::CubeMapInner,
    texture::{MipType, TextureInner},
};

use super::{
    allocator::{ResourceAllocator, ResourceId},
    meshes::{MeshBlock, MeshBuffers},
};

// TODO: Make this configurable.
const MAX_UPLOAD_COUNT: usize = 64;

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
    TextureMip {
        id: ResourceId,
        _dst: crate::texture::Texture,
        mip_level: usize,
        staging_buffer: Buffer,
    },
    CubeMapMip {
        id: ResourceId,
        _dst: crate::cube_map::CubeMap,
        mip_level: usize,
        staging_buffer: Buffer,
    },
    CubeMap {
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
    TextureMip { mip_level: usize, id: ResourceId },
    CubeMapMip { mip_level: usize, id: ResourceId },
    CubeMap(ResourceId),
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
        loop {
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

            if !(blocking && to_remove.len() != self.uploads.len()) {
                break;
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
        cube_maps: &mut ResourceAllocator<CubeMapInner>,
    ) {
        if self.pending.is_empty() {
            return;
        }

        let mut resources = Vec::default();
        let mut transfer_commands = self.ctx.transfer().command_buffer();
        let mut main_commands = None;

        let mut upload_count = 0;
        for request in &self.pending {
            if upload_count >= MAX_UPLOAD_COUNT {
                break;
            }
            upload_count += 1;

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
                    let mut cur_vertex_offset = 0;
                    let vbs = mesh_buffers.get_vertex_buffer(*layout).unwrap();

                    let mut copy_to_buffer = |elem_size: usize, element: VertexLayout| {
                        let copy_len = (*vertex_count * elem_size) as u64;
                        transfer_commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                            src: vertex_staging,
                            src_array_element: 0,
                            src_offset: cur_vertex_offset,
                            dst: vbs.buffer(element).unwrap(),
                            dst_array_element: 0,
                            dst_offset: vertex_dst.base() as u64 * elem_size as u64,
                            len: copy_len,
                        });
                        cur_vertex_offset += copy_len;
                    };

                    // Position
                    copy_to_buffer(std::mem::size_of::<Vec4>(), VertexLayout::empty());

                    if layout.contains(VertexLayout::NORMAL) {
                        copy_to_buffer(std::mem::size_of::<Vec4>(), VertexLayout::NORMAL);
                    }
                    if layout.contains(VertexLayout::TANGENT) {
                        copy_to_buffer(std::mem::size_of::<Vec4>(), VertexLayout::TANGENT);
                    }
                    if layout.contains(VertexLayout::COLOR) {
                        copy_to_buffer(std::mem::size_of::<Vec4>(), VertexLayout::COLOR);
                    }
                    if layout.contains(VertexLayout::UV0) {
                        copy_to_buffer(std::mem::size_of::<Vec2>(), VertexLayout::UV0);
                    }
                    if layout.contains(VertexLayout::UV1) {
                        copy_to_buffer(std::mem::size_of::<Vec2>(), VertexLayout::UV1);
                    }
                    if layout.contains(VertexLayout::UV2) {
                        copy_to_buffer(std::mem::size_of::<Vec2>(), VertexLayout::UV2);
                    }
                    if layout.contains(VertexLayout::UV3) {
                        copy_to_buffer(std::mem::size_of::<Vec2>(), VertexLayout::UV3);
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
                            commands.blit(
                                BlitSource::Texture(&dst.texture),
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
                StagingRequest::TextureMip {
                    id,
                    mip_level,
                    staging_buffer,
                    ..
                } => {
                    let dst = match textures.get(*id) {
                        Some(tex) => tex,
                        // Texture was dropped so upload is no longer needed
                        None => continue,
                    };
                    let (width, height, _) = dst.texture.dims();

                    transfer_commands.copy_buffer_to_texture(
                        &dst.texture,
                        staging_buffer,
                        BufferTextureCopy {
                            buffer_offset: 0,
                            buffer_row_length: 0,
                            buffer_image_height: 0,
                            buffer_array_element: 0,
                            texture_offset: (0, 0, 0),
                            texture_extent: (
                                width.shr(mip_level).max(1),
                                height.shr(mip_level).max(1),
                                1,
                            ),
                            texture_mip_level: *mip_level,
                            texture_array_element: 0,
                        },
                    );

                    StagingResource::TextureMip {
                        mip_level: *mip_level,
                        id: *id,
                    }
                }
                StagingRequest::CubeMapMip {
                    id,
                    mip_level,
                    staging_buffer,
                    ..
                } => {
                    let dst = match cube_maps.get(*id) {
                        Some(cm) => cm,
                        // Cube map was dropped so upload is no longer needed
                        None => continue,
                    };

                    transfer_commands.copy_buffer_to_cube_map(
                        &dst.cube_map,
                        staging_buffer,
                        BufferCubeMapCopy {
                            buffer_offset: 0,
                            buffer_array_element: 0,
                            cube_map_mip_level: *mip_level,
                            cube_map_array_element: 0,
                        },
                    );

                    StagingResource::CubeMapMip {
                        mip_level: *mip_level,
                        id: *id,
                    }
                }
                StagingRequest::CubeMap {
                    id,
                    staging_buffer,
                    mip_type,
                } => match *mip_type {
                    MipType::Generate => {
                        todo!()
                    }
                    MipType::Upload => {
                        let dst = match cube_maps.get(*id) {
                            Some(cube_map) => cube_map,
                            // Cube map was dropped so upload is no longer needed
                            None => continue,
                        };

                        let mip_level = dst.mip_levels.saturating_sub(1) as usize;

                        transfer_commands.copy_buffer_to_cube_map(
                            &dst.cube_map,
                            staging_buffer,
                            BufferCubeMapCopy {
                                buffer_offset: 0,
                                buffer_array_element: 0,
                                cube_map_mip_level: mip_level,
                                cube_map_array_element: 0,
                            },
                        );

                        StagingResource::CubeMap(*id)
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
        self.pending.drain(..upload_count);

        self.uploads.push(Upload {
            transfer_job,
            main_job,
            resources,
        });
    }
}
