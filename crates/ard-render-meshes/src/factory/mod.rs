pub mod allocator;
pub mod vertex_buffers;

use ard_render_base::{ecs::Frame, resource::ResourceId};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use ard_formats::mesh::{IndexData, VertexAttribute, VertexLayout};
use ard_pal::prelude::*;
use ard_render_si::types::GpuMeshInfo;

use self::{
    allocator::{BufferBlock, BufferBlockAllocator},
    vertex_buffers::VertexBuffers,
};

#[derive(Serialize, Deserialize)]
pub struct MeshFactoryConfig {
    /// Number of vertex elements held in the smallest block of the vertex buffer. Must be a power
    /// of 2.
    pub base_vertex_block_len: usize,
    /// Number of indices held in the smallest block of the index buffer. Must be a power of 2.
    pub base_index_block_len: usize,
    /// Maps individual vertex layouts to their default sizes.
    pub default_vertex_layout_len: HashMap<VertexLayout, usize>,
    /// All vertex layouts not defined in `default_vertex_layout_len` will have this default
    /// length.
    pub default_vertex_buffer_len: usize,
    /// Default length of the shared index buffer.
    pub default_index_buffer_len: usize,
}

pub struct MeshFactory {
    ctx: Context,
    config: MeshFactoryConfig,
    /// Maps vertex layouts to the allocators for meshes that have that layout.
    vertex_allocators: FxHashMap<VertexLayout, VertexBuffers>,
    /// Allocator for index data.
    index_allocator: BufferBlockAllocator,
    /// SSBO for mesh info.
    mesh_info_buffer: Buffer,
    /// Staging for mesh info upload. One list per frame in flight.
    mesh_info_staging: Vec<Vec<(ResourceId, GpuMeshInfo)>>,
}

/// Data upload information for a mesh.
pub struct MeshUpload {
    pub vertex_staging: Buffer,
    pub vertex_offsets: HashMap<VertexAttribute, u32>,
    pub index_staging: Buffer,
    pub vertex_count: usize,
    pub block: MeshBlock,
}

/// Mesh allocated from a `MeshFactory`.
#[derive(Debug, Copy, Clone)]
pub struct MeshBlock {
    layout: VertexLayout,
    vb: BufferBlock,
    ib: BufferBlock,
}

impl MeshFactory {
    pub fn new(
        ctx: Context,
        config: MeshFactoryConfig,
        max_mesh_count: usize,
        frames_in_flight: usize,
    ) -> Self {
        let index_allocator = BufferBlockAllocator::new(
            ctx.clone(),
            Some("index_buffer".into()),
            BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST | BufferUsage::TRANSFER_SRC,
            config.base_index_block_len,
            config.default_index_buffer_len,
            IndexData::SIZE,
        );

        Self {
            ctx: ctx.clone(),
            config,
            vertex_allocators: FxHashMap::default(),
            index_allocator,
            mesh_info_buffer: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (std::mem::size_of::<GpuMeshInfo>() * max_mesh_count) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("mesh_info".into()),
                },
            )
            .unwrap(),
            mesh_info_staging: (0..frames_in_flight).map(|_| Vec::default()).collect(),
        }
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
    pub fn vertex_buffer(&self, layout: VertexLayout) -> Option<&VertexBuffers> {
        self.vertex_allocators.get(&layout)
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
    ) -> MeshBlock {
        let vbs = self.vertex_allocators.entry(layout).or_insert_with(|| {
            VertexBuffers::new(
                &self.ctx,
                layout,
                self.config.base_vertex_block_len,
                *self
                    .config
                    .default_vertex_layout_len
                    .get(&layout)
                    .unwrap_or(&self.config.default_vertex_buffer_len),
            )
        });

        let vb = match vbs.allocate(vertex_count) {
            Some(vb) => vb,
            None => {
                vbs.reserve(vertex_count);
                vbs.allocate(vertex_count)
                    .expect("vertex buffer reservation should allow for further allocation")
            }
        };

        let ib = match self.index_allocator.allocate(index_count) {
            Some(ib) => ib,
            None => {
                self.index_allocator.reserve(index_count);
                self.index_allocator
                    .allocate(index_count)
                    .expect("index buffer reservation should allow for further allocation")
            }
        };

        MeshBlock { layout, vb, ib }
    }

    /// Free an allocated mesh block.
    pub fn free(&mut self, block: MeshBlock) {
        self.index_allocator.free(block.ib);
        if let Some(vbs) = self.vertex_allocators.get_mut(&block.layout) {
            vbs.free(block.vb);
        }
    }

    /// Flushes all mesh info to the GPU for a particular frame in flight.
    pub fn flush_mesh_info(&mut self, frame: Frame) {
        let staging = &mut self.mesh_info_staging[usize::from(frame)];

        let mut view = self.mesh_info_buffer.write(usize::from(frame)).unwrap();

        staging.drain(..).for_each(|(id, info)| {
            view.set_as_array(info, usize::from(id));
        });
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
            dst_offset: upload.block.index_block().base() as u64 * IndexData::SIZE as u64,
            len: upload.index_staging.size(),
        });

        // Copy in vertex data
        // NOTE: Safe to unwrap since the vertex buffers for this layout must have existed to
        // allocate the mesh
        let vbs = self.vertex_allocators.get(&upload.block.layout()).unwrap();

        // Copy in vertex attributes
        for (attribute, offset) in &upload.vertex_offsets {
            commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                src: &upload.vertex_staging,
                src_array_element: 0,
                src_offset: *offset as u64,
                // Safe to unrap since the allocator has a matching layout
                dst: vbs.buffer(*attribute).unwrap(),
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
    pub fn layout(&self) -> VertexLayout {
        self.layout
    }
}
