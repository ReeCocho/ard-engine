use ard_formats::vertex::{VertexAttribute, VertexLayout};
use ard_pal::prelude::*;

use static_assertions::const_assert_eq;

use super::allocator::{BufferBlock, BufferBlockAllocator};

pub struct VertexBuffers {
    allocators: [BufferBlockAllocator; VertexAttribute::COUNT],
}

impl VertexBuffers {
    pub(super) fn new(ctx: &Context, base_block_len: usize, block_count: usize) -> Self {
        let allocators = std::array::from_fn(|i| {
            let attribute: VertexAttribute = VertexLayout::from_bits(1u8 << i as u8)
                .unwrap()
                .try_into()
                .unwrap();
            BufferBlockAllocator::new(
                ctx.clone(),
                Some(format!("vb_{attribute:?}")),
                BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
                base_block_len,
                block_count,
                attribute.size(),
            )
        });

        Self { allocators }
    }

    #[inline(always)]
    pub fn buffer(&self, attribute: VertexAttribute) -> &Buffer {
        self.allocators[attribute.idx()].buffer()
    }

    #[inline]
    pub fn allocate(&mut self, count: usize) -> Option<BufferBlock> {
        // Buffer 0 is the position buffer which always exists. Since the state of all allocators
        // is the same, if this fails all other ones will also fail and need expansion. If it
        // succeeds, then all allocated blocks will be the same.
        const_assert_eq!(VertexAttribute::Position.idx(), 0);

        if let Some(block) = self.allocators[0].allocate(count) {
            self.allocators.iter_mut().skip(1).for_each(|a| {
                a.allocate(count);
            });
            Some(block)
        } else {
            None
        }
    }

    #[inline]
    pub fn free(&mut self, block: BufferBlock) {
        self.allocators.iter_mut().for_each(|a| a.free(block));
    }

    /// Given a number of vertices to allocate, creates a new level such that the newest max level
    /// can fit all the vertices.
    pub fn reserve(&mut self, vertex_count: usize) {
        self.allocators
            .iter_mut()
            .for_each(|a| a.reserve(vertex_count));
    }
}
