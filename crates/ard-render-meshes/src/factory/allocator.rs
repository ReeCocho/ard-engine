use ard_alloc::buddy::{BuddyAllocator, BuddyBlock};
use ard_log::warn;
use ard_pal::prelude::*;

/// A buddy block allocator used for allocating vertex and index data.
pub struct BufferBlockAllocator {
    ctx: Context,
    /// Debug name used by the buffer.
    debug_name: Option<String>,
    /// Actual GPU buffer with the data.
    buffer: Buffer,
    /// The underlying buddy block allocator.
    alloc: BuddyAllocator,
    /// Size of objects allocated.
    object_size: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BufferBlock(BuddyBlock);

impl BufferBlockAllocator {
    pub fn new(
        ctx: Context,
        debug_name: Option<String>,
        usage: BufferUsage,
        base_block_cap: usize,
        block_count: usize,
        object_size: usize,
    ) -> Self {
        let buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (object_size * base_block_cap * block_count) as u64,
                array_elements: 1,
                buffer_usage: usage,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Concurrent,
                debug_name: debug_name.clone(),
            },
        )
        .unwrap();

        Self {
            ctx,
            debug_name,
            buffer,
            alloc: BuddyAllocator::new(base_block_cap, block_count),
            object_size,
        }
    }

    #[inline(always)]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Allocate a region of the buffer to fit `count` number of elements.
    ///
    /// Returns `None` if allocation failed.
    pub fn allocate(&mut self, count: usize) -> Option<BufferBlock> {
        self.alloc.allocate(count).map(BufferBlock)
    }

    /// Frees an allocated block.
    ///
    /// ## Note
    /// There is no check to prevent you from freeing a block that was not allocated with this
    /// allocator, so keep that in mind.
    pub fn free(&mut self, block: BufferBlock) {
        self.alloc.free(block.0);
    }

    /// Expands the buffer to fit a new number of blocks. Copies data from the original buffer to
    /// the new buffer if resize occurs. No-op if the new block length is smaller than the
    /// original.
    fn expand(&mut self, new_block_count: usize) {
        self.alloc.expand(new_block_count);

        // Create new buffer
        let new_buffer = Buffer::new(
            self.ctx.clone(),
            BufferCreateInfo {
                size: (self.object_size * self.alloc.base_block_cap() * self.alloc.block_count())
                    as u64,
                array_elements: 1,
                buffer_usage: self.buffer.buffer_usage(),
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Concurrent,
                debug_name: self.debug_name.clone(),
            },
        )
        .unwrap();

        // Warn the user that this is slow
        warn!(
            "GPU buffer allocator `{:?}` expanded. \
            This operation stalls rendering. \
            Consider making the default size of this allocator larger.",
            self.debug_name
        );

        // Record copy command
        let mut commands = self.ctx.transfer().command_buffer();
        commands.copy_buffer_to_buffer(CopyBufferToBuffer {
            src: &self.buffer,
            src_array_element: 0,
            src_offset: 0,
            dst: &new_buffer,
            dst_array_element: 0,
            dst_offset: 0,
            len: self.buffer.size(),
        });
        self.ctx
            .transfer()
            .submit(Some("resize_vertex_buffer"), commands);

        // Swap old and new buffer
        self.buffer = new_buffer;
    }

    /// Ensures the capacity of the allocator can accomodate the provided array length.
    pub fn reserve(&mut self, count: usize) {
        self.alloc.reserve_for(count);
        self.expand(self.alloc.block_count());
    }
}

impl BufferBlock {
    /// Base pointer (measured in T's) for this block.
    #[inline]
    pub fn base(&self) -> u32 {
        self.0.base()
    }

    /// Number of T's that can fit in this block.
    #[inline]
    pub fn len(&self) -> u32 {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}
