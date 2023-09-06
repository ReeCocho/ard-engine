use ard_log::warn;
use ard_pal::prelude::*;
use fxhash::FxHashSet;

/// A buddy block allocator used for allocating vertex and index data.
pub struct BufferBlockAllocator {
    ctx: Context,
    /// Debug name used by the buffer.
    debug_name: Option<String>,
    /// Actual GPU buffer with the data.
    buffer: Buffer,
    /// Free blocks for each level of the allocator.
    free_blocks: Vec<FxHashSet<BufferBlock>>,
    /// The number of objects that can fit in the smallest block size.
    base_block_len: usize,
    /// Total number of base blocks. Must be a power of 2.
    block_count: usize,
    /// Size of objects allocated.
    object_size: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BufferBlock {
    base: u32,
    len: u32,
}

impl BufferBlockAllocator {
    pub fn new(
        ctx: Context,
        debug_name: Option<String>,
        usage: BufferUsage,
        base_block_len: usize,
        block_count: usize,
        object_size: usize,
    ) -> Self {
        assert!(block_count.is_power_of_two());

        let order = (block_count as f32).log2() as usize + 1;
        let mut free_blocks = Vec::with_capacity(order);
        free_blocks.resize(order, FxHashSet::<BufferBlock>::default());
        free_blocks[order - 1].insert(BufferBlock {
            base: 0,
            len: (base_block_len * block_count) as u32,
        });

        let buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (object_size * base_block_len * block_count) as u64,
                array_elements: 1,
                buffer_usage: usage,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: debug_name.clone(),
            },
        )
        .unwrap();

        Self {
            ctx,
            debug_name,
            buffer,
            free_blocks,
            block_count,
            base_block_len,
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
        // Determine what level the allocation must be placed it
        let block_count =
            (count / self.base_block_len) + usize::from(count % self.base_block_len != 0);
        let level = (block_count as f32).log2().ceil() as usize;

        // Too big
        if level >= self.free_blocks.len() {
            return None;
        }

        // Ensure there is a free block at the current level
        let mut upper_level = level;
        while self.free_blocks[upper_level].is_empty() {
            upper_level += 1;

            // No free blocks
            if upper_level >= self.free_blocks.len() {
                return None;
            }
        }

        // Split the current block until we're at the desired level
        while upper_level != level {
            let block = *self.free_blocks[upper_level].iter().next().unwrap();
            self.free_blocks[upper_level].remove(&block);

            upper_level -= 1;
            let new_len = block.len / 2;

            let left_block = BufferBlock {
                base: block.base,
                len: new_len,
            };

            let right_block = BufferBlock {
                base: block.base + new_len,
                len: new_len,
            };

            self.free_blocks[upper_level].insert(right_block);
            self.free_blocks[upper_level].insert(left_block);
        }

        // Grab the free block
        let block = *self.free_blocks[level].iter().next().unwrap();
        self.free_blocks[level].remove(&block);

        Some(block)
    }

    /// Frees an allocated block.
    ///
    /// ## Note
    /// There is no check to prevent you from freeing a block that was not allocated with this
    /// allocator, so keep that in mind.
    pub fn free(&mut self, mut block: BufferBlock) {
        let mut level = ((block.len as usize / self.base_block_len) as f32).log2() as usize;
        let mut is_even =
            ((block.base / self.base_block_len as u32) / (1 << level as u32)) % 2 == 0;

        // If this covers the whole allocation range, no need for merging
        if level == self.free_blocks.len() {
            self.free_blocks[level].insert(block);
            return;
        }

        // Insert into free list
        self.free_blocks[level].insert(block);

        // Continue to merge until we've either hit the max level or have no more buddy
        while let Some(buddy) = self.free_blocks[level].take(&BufferBlock {
            base: if is_even {
                block.base + block.len
            } else {
                block.base - block.len
            },
            len: block.len,
        }) {
            // Take ourselves out of the list
            self.free_blocks[level].remove(&block);

            // Generate the merged block
            block = BufferBlock {
                base: if is_even { block.base } else { buddy.base },
                len: block.len * 2,
            };

            // Insert merged block into the next level
            self.free_blocks[level + 1].insert(block);

            // Update level and check if we've reached the max
            level += 1;
            if level == self.free_blocks.len() - 1 {
                return;
            }

            is_even = ((block.base / self.base_block_len as u32) / (1 << level as u32)) % 2 == 0;
        }
    }

    /// Expands the buffer to fit a new number of blocks. Copies data from the original buffer to
    /// the new buffer if resize occurs. No-op if the new block length is smaller than the
    /// original.
    fn expand(&mut self, new_block_count: usize) {
        assert!(new_block_count.is_power_of_two());

        if new_block_count < self.block_count {
            return;
        }

        let new_order = (new_block_count as f32).log2() as usize + 1;

        // If we have nothing allocated yet, just clear the current level and update the new block
        if !self.free_blocks[self.free_blocks.len() - 1].is_empty() {
            self.free_blocks.last_mut().unwrap().clear();
            self.free_blocks
                .resize(new_order, FxHashSet::<BufferBlock>::default());
            self.free_blocks.last_mut().unwrap().insert(BufferBlock {
                base: 0,
                len: (new_block_count * self.base_block_len) as u32,
            });
        }
        // Things are allocated. Just add a new "right-most" block that is free to each new level
        else {
            let old_order = self.free_blocks.len() - 1;
            self.free_blocks
                .resize(new_order, FxHashSet::<BufferBlock>::default());

            for level in old_order..(new_order - 1) {
                self.free_blocks[level].insert(BufferBlock {
                    base: ((1 << level) * self.base_block_len) as u32,
                    len: ((1 << level) * self.base_block_len) as u32,
                });
            }
        }

        self.block_count = new_block_count;

        // Create new buffer
        let new_buffer = Buffer::new(
            self.ctx.clone(),
            BufferCreateInfo {
                size: (self.object_size * self.base_block_len * self.block_count) as u64,
                array_elements: 1,
                buffer_usage: self.buffer.buffer_usage(),
                memory_usage: MemoryUsage::GpuOnly,
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
        // Compute what level is needed to accomodate the length
        let needed_level = (count as f32 / self.base_block_len as f32)
            .ceil()
            .log2()
            .ceil() as usize;

        // If our needed level is smaller than the current max level, all we need is one more level
        if needed_level < self.free_blocks.len() {
            self.expand(self.block_count * 2);
        }
        // If our needed level is larger than the current max level, we need to expand to however
        // many levels is requested by needed_level
        else {
            self.expand(1 << (needed_level + 1));
        }
    }
}

impl BufferBlock {
    /// Base pointer (measured in T's) for this block.
    #[inline]
    pub fn base(&self) -> u32 {
        self.base
    }

    /// Number of T's that can fit in this block.
    #[allow(dead_code)]
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }
}
