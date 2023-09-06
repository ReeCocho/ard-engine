use ard_formats::mesh::{VertexAttribute, VertexLayout};
use ard_pal::prelude::*;

use static_assertions::const_assert_eq;
use thiserror::*;

use super::allocator::{BufferBlock, BufferBlockAllocator};

pub struct VertexBuffers {
    allocators: [Option<BufferBlockAllocator>; VertexAttribute::COUNT],
}

#[derive(Debug, Error, Copy, Clone)]
#[error("vertex buffer missing attribute `{0:?}`")]
pub struct VertexBufferBindError(pub VertexLayout);

impl VertexBuffers {
    pub(super) fn new(
        ctx: &Context,
        layout: VertexLayout,
        base_block_len: usize,
        block_count: usize,
    ) -> Self {
        // Ensure we have the required attributes
        assert!(layout.contains(VertexLayout::POSITION | VertexLayout::NORMAL));

        let mut allocators: [Option<BufferBlockAllocator>; VertexAttribute::COUNT] =
            Default::default();

        for bit in layout.iter() {
            // Safe to unwrap since bits map one-to-one with attributes
            let attribute: VertexAttribute = bit.try_into().unwrap();
            allocators[attribute.idx()] = Some(BufferBlockAllocator::new(
                ctx.clone(),
                Some(format!("vb_{layout:?}_{attribute:?}")),
                BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
                base_block_len,
                block_count,
                attribute.size(),
            ));
        }

        Self { allocators }
    }

    /// Binds the vertex buffers to a particular render pass.
    ///
    /// The target layout must be the same as or a subset of the layout of the contained vertex
    /// buffers.
    pub fn bind<'a>(
        &'a self,
        render_pass: &mut RenderPass<'a>,
        target_layout: VertexLayout,
    ) -> Result<(), VertexBufferBindError> {
        let mut binds = Vec::with_capacity(target_layout.bits().count_ones() as usize);

        for bit in target_layout.iter() {
            // Safe to unwrap since bits map one-to-one with attributes
            let attribute: VertexAttribute = bit.try_into().unwrap();
            binds.push(VertexBind {
                buffer: match &self.allocators[attribute.idx()] {
                    Some(alloc) => alloc.buffer(),
                    None => return Err(VertexBufferBindError(bit)),
                },
                array_element: 0,
                offset: 0,
            });
        }

        // Perform the bind
        render_pass.bind_vertex_buffers(0, binds);

        Ok(())
    }

    #[inline(always)]
    pub fn buffer(&self, attribute: VertexAttribute) -> Option<&Buffer> {
        self.allocators[attribute.idx()]
            .as_ref()
            .map(|a| a.buffer())
    }

    #[inline]
    pub fn allocate(&mut self, count: usize) -> Option<BufferBlock> {
        // Buffer 0 is the position buffer which always exists. Since the state of all allocators
        // is the same, if this fails all other ones will also fail and need expansion. If it
        // succeeds, then all allocated blocks will be the same.
        const_assert_eq!(VertexAttribute::Position.idx(), 0);

        if let Some(block) = self.allocators[0].as_mut().unwrap().allocate(count) {
            self.allocators
                .iter_mut()
                .skip(1)
                .filter_map(|a| a.as_mut())
                .for_each(|a| {
                    a.allocate(count);
                });
            Some(block)
        } else {
            None
        }
    }

    #[inline]
    pub fn free(&mut self, block: BufferBlock) {
        self.allocators
            .iter_mut()
            .filter_map(|a| a.as_mut())
            .for_each(|a| a.free(block));
    }

    /// Given a number of vertices to allocate, creates a new level such that the newest max level
    /// can fit all the vertices.
    pub fn reserve(&mut self, vertex_count: usize) {
        self.allocators
            .iter_mut()
            .filter_map(|a| a.as_mut())
            .for_each(|a| a.reserve(vertex_count));
    }
}
