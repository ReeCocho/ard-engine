pub mod allocator;
pub mod vertex_buffers;

use ard_formats::{
    mesh::MeshData,
    vertex::{VertexAttribute, VertexLayout},
};
use ard_render_base::{ecs::Frame, resource::ResourceId, FRAMES_IN_FLIGHT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use ard_pal::prelude::*;
use ard_render_si::{bindings::*, types::*};

use self::{
    allocator::{BufferBlock, BufferBlockAllocator},
    vertex_buffers::VertexBuffers,
};

#[derive(Serialize, Deserialize)]
pub struct MeshFactoryConfig {
    /// Number of vertex elements held in the smallest block of the vertex buffer.
    pub base_vertex_block_len: usize,
    /// Number of indices held in the smallest block of the index buffer.
    pub base_index_block_len: usize,
    /// Number of meshlet held in the smallest block of the meshlet buffer.
    pub base_meshlet_block_len: usize,
    /// Number of vertices held in the smallest block of the vertex buffer. Must be a power of 2.
    pub default_vertex_buffer_len: usize,
    /// Number of indices held in the smallest block of the index buffer. Must be a power of 2.
    pub default_index_buffer_len: usize,
    /// Default number of blocks in the meshlet buffer. Must be a power of 2.
    pub default_meshlet_buffer_len: usize,
}

pub struct MeshFactory {
    _ctx: Context,
    /// Allocator for vertex data.
    vertex_allocator: VertexBuffers,
    /// Allocator for index data.
    index_allocator: BufferBlockAllocator,
    /// Allocator for mesh meshlet data.
    meshlet_allocator: BufferBlockAllocator,
    /// SSBO for mesh info.
    mesh_info_buffer: Buffer,
    /// Staging for mesh info upload. One list per frame in flight.
    mesh_info_staging: [Vec<(ResourceId, GpuMeshInfo)>; FRAMES_IN_FLIGHT],
    /// Descriptor set for vertex, index, and meshlet data.
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    needs_rebind: [bool; FRAMES_IN_FLIGHT],
}

/// Data upload information for a mesh.
pub struct MeshUpload {
    pub vertex_staging: Buffer,
    pub vertex_offsets: HashMap<VertexAttribute, u32>,
    pub index_staging: Buffer,
    pub vertex_count: usize,
    pub meshlet_staging: Buffer,
    pub meshlet_count: usize,
    pub block: MeshBlock,
}

/// Mesh allocated from a `MeshFactory`.
#[derive(Debug, Copy, Clone)]
pub struct MeshBlock {
    layout: VertexLayout,
    vb: BufferBlock,
    ib: BufferBlock,
    mb: BufferBlock,
}

impl MeshFactory {
    pub fn new(
        ctx: Context,
        layouts: &Layouts,
        config: MeshFactoryConfig,
        max_mesh_count: usize,
    ) -> Self {
        let index_allocator = BufferBlockAllocator::new(
            ctx.clone(),
            Some("index_buffer".into()),
            BufferUsage::STORAGE_BUFFER
                | BufferUsage::ACCELERATION_STRUCTURE_READ
                | BufferUsage::TRANSFER_DST,
            config.base_index_block_len,
            config.default_index_buffer_len,
            MeshData::INDEX_SIZE,
        );

        let meshlet_allocator = BufferBlockAllocator::new(
            ctx.clone(),
            Some("meshlet_buffer".into()),
            BufferUsage::STORAGE_BUFFER
                | BufferUsage::ACCELERATION_STRUCTURE_READ
                | BufferUsage::TRANSFER_DST,
            config.base_meshlet_block_len,
            config.default_meshlet_buffer_len,
            std::mem::size_of::<GpuMeshlet>(),
        );

        let vertex_allocator = VertexBuffers::new(
            &ctx,
            config.base_index_block_len,
            config.default_index_buffer_len,
        );

        let sets = std::array::from_fn(|_| {
            DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.mesh_data.clone(),
                    debug_name: Some("mesh_data_set".into()),
                },
            )
            .unwrap()
        });

        Self {
            _ctx: ctx.clone(),
            vertex_allocator,
            index_allocator,
            meshlet_allocator,
            mesh_info_buffer: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (std::mem::size_of::<GpuMeshInfo>() * max_mesh_count) as u64,
                    array_elements: FRAMES_IN_FLIGHT,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Concurrent,
                    debug_name: Some("mesh_info".into()),
                },
            )
            .unwrap(),
            mesh_info_staging: Default::default(),
            sets,
            needs_rebind: std::array::from_fn(|_| true),
        }
    }

    #[inline(always)]
    pub fn mesh_data_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }

    #[inline(always)]
    pub fn mesh_info_buffer(&self) -> &Buffer {
        &self.mesh_info_buffer
    }

    #[inline(always)]
    pub fn index_buffer(&self) -> &Buffer {
        self.index_allocator.buffer()
    }

    #[inline(always)]
    pub fn vertex_buffer(&self) -> &VertexBuffers {
        &self.vertex_allocator
    }

    /// Set the info for a particular mesh.
    pub fn set_mesh_info(&mut self, id: ResourceId, info: GpuMeshInfo) {
        self.mesh_info_staging
            .iter_mut()
            .for_each(|list| list.push((id, info)));
    }

    /// Allocate a region for a mesh with the provided layout and sizes.
    pub fn allocate(
        &mut self,
        layout: VertexLayout,
        vertex_count: usize,
        index_count: usize,
        meshlet_count: usize,
    ) -> MeshBlock {
        let vb = match self.vertex_allocator.allocate(vertex_count) {
            Some(vb) => vb,
            None => {
                panic!("ran out of vertex memory");
                /*
                self.needs_rebind.iter_mut().for_each(|r| *r = true);
                self.vertex_allocator.reserve(vertex_count);
                self.vertex_allocator
                    .allocate(vertex_count)
                    .expect("vertex buffer reservation should allow for further allocation")
                */
            }
        };

        let ib = match self.index_allocator.allocate(index_count) {
            Some(ib) => ib,
            None => {
                panic!("ran out of index memory");
                /*
                self.needs_rebind.iter_mut().for_each(|r| *r = true);
                self.index_allocator.reserve(index_count);
                self.index_allocator
                    .allocate(index_count)
                    .expect("index buffer reservation should allow for further allocation")
                */
            }
        };

        let mb = match self.meshlet_allocator.allocate(meshlet_count) {
            Some(ib) => ib,
            None => {
                panic!("ran out of meshlet memory");
                /*
                self.needs_rebind.iter_mut().for_each(|r| *r = true);
                self.meshlet_allocator.reserve(meshlet_count);
                self.meshlet_allocator
                    .allocate(meshlet_count)
                    .expect("meshlet buffer reservation should allow for further allocation")
                */
            }
        };

        MeshBlock { layout, vb, ib, mb }
    }

    /// Free an allocated mesh block.
    pub fn free(&mut self, block: MeshBlock) {
        self.index_allocator.free(block.ib);
        self.vertex_allocator.free(block.vb);
        self.meshlet_allocator.free(block.mb);
    }

    /// Flushes all mesh info to the GPU for a particular frame in flight.
    pub fn flush_mesh_info(&mut self, frame: Frame) {
        let staging = &mut self.mesh_info_staging[usize::from(frame)];

        let mut view = self.mesh_info_buffer.write(usize::from(frame)).unwrap();

        staging.drain(..).for_each(|(id, info)| {
            view.set_as_array(info, usize::from(id));
        });
    }

    /// Checks if descriptor sets need rebinding.
    pub fn check_rebind(&mut self, frame: Frame) {
        let frame = usize::from(frame);
        if !self.needs_rebind[frame] {
            return;
        }

        self.sets[frame].update(&[
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_POSITIONS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.vertex_allocator.buffer(VertexAttribute::Position),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_NORMALS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.vertex_allocator.buffer(VertexAttribute::Normal),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_TANGENTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.vertex_allocator.buffer(VertexAttribute::Tangent),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_UV_0_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.vertex_allocator.buffer(VertexAttribute::Uv0),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_UV_1_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.vertex_allocator.buffer(VertexAttribute::Uv1),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_MESHLETS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.meshlet_allocator.buffer(),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_INDICES_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: self.index_allocator.buffer(),
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: MESH_DATA_SET_MESH_INFO_LOOKUP_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.mesh_info_buffer,
                    array_element: frame,
                },
            },
        ]);

        self.needs_rebind[frame] = false;
    }

    /// Records a command to upload a mesh to the factory.
    ///
    /// ## Note
    /// `commands` must have transfer operation support.
    pub fn upload<'a>(&'a self, commands: &mut CommandBuffer<'a>, upload: &'a MeshUpload) {
        // Copy index data
        commands.copy_buffer_to_buffer(CopyBufferToBuffer {
            src: &upload.index_staging,
            src_array_element: 0,
            src_offset: 0,
            dst: self.index_allocator.buffer(),
            dst_array_element: 0,
            dst_offset: upload.block.index_block().base() as u64 * MeshData::INDEX_SIZE as u64,
            len: upload.index_staging.size(),
        });

        // Copy meshlet data
        commands.copy_buffer_to_buffer(CopyBufferToBuffer {
            src: &upload.meshlet_staging,
            src_array_element: 0,
            src_offset: 0,
            dst: self.meshlet_allocator.buffer(),
            dst_array_element: 0,
            dst_offset: upload.block.meshlet_block().base() as u64
                * std::mem::size_of::<GpuMeshlet>() as u64,
            len: upload.meshlet_staging.size(),
        });

        // Copy in vertex attributes
        for (attribute, offset) in &upload.vertex_offsets {
            commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                src: &upload.vertex_staging,
                src_array_element: 0,
                src_offset: *offset as u64,
                // Safe to unrap since the allocator has a matching layout
                dst: self.vertex_allocator.buffer(*attribute),
                dst_array_element: 0,
                dst_offset: upload.block.vertex_block().base() as u64 * attribute.size() as u64,
                len: (upload.vertex_count * attribute.size()) as u64,
            });
        }
    }
}

impl MeshBlock {
    #[inline(always)]
    pub fn vertex_block(&self) -> BufferBlock {
        self.vb
    }

    #[inline(always)]
    pub fn index_block(&self) -> BufferBlock {
        self.ib
    }

    #[inline(always)]
    pub fn meshlet_block(&self) -> BufferBlock {
        self.mb
    }

    #[inline(always)]
    pub fn layout(&self) -> VertexLayout {
        self.layout
    }
}
