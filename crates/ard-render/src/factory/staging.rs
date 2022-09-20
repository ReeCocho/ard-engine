use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;

use crate::mesh::VertexLayout;

use super::{
    allocator::ResourceId,
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
}

pub(crate) struct Staging {
    ctx: Context,
    uploads: Vec<Upload>,
    pending: Vec<StagingRequest>,
}

struct Upload {
    job: Job,
    resources: Vec<StagingResource>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StagingResource {
    Mesh(ResourceId),
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
            if upload.job.poll_status() == JobStatus::Complete {
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
    pub fn upload(&mut self, mesh_buffers: &mut MeshBuffers) {
        if self.pending.is_empty() {
            return;
        }

        let mut resources = Vec::default();
        let job = self.ctx.transfer().submit(Some("staging"), |commands| {
            for request in &mut self.pending {
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
                        commands.copy_buffer_to_buffer(CopyBufferToBuffer {
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
                            commands.copy_buffer_to_buffer(CopyBufferToBuffer {
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

                        StagingResource::Mesh(*id)
                    }
                };
                resources.push(resc);
            }
        });

        self.uploads.push(Upload { job, resources });
    }
}
