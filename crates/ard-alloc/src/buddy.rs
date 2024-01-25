use rustc_hash::FxHashSet;

/// A simple resizable buddy block allocator.
///
/// This allocator is a sort of "virtual allocator". By that, I mean that it doesn't actually
/// manage the memory itself. Instead, it gives you back blocks which you can use to define
/// subregions of allocated memory that you can manage yourself.
pub struct BuddyAllocator {
    /// Free blocks for each level of the allocator.
    free_blocks: Vec<FxHashSet<BuddyBlock>>,
    /// The number of objects that can fit in the smallest block size.
    base_block_cap: usize,
    /// Total number of base blocks. Must be a power of 2.
    block_count: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BuddyBlock {
    base: u32,
    len: u32,
}

impl BuddyAllocator {
    /// Creates a new allocator.
    ///
    /// `base_block_cap` is the number of elements that can be stored in the smallest block size.
    /// `block_count` is the default number of blocks, and must be a power of two.
    pub fn new(base_block_cap: usize, block_count: usize) -> Self {
        assert!(block_count.is_power_of_two());

        let order = (block_count as f32).log2() as usize + 1;
        let mut free_blocks = Vec::with_capacity(order);
        free_blocks.resize(order, FxHashSet::<BuddyBlock>::default());
        free_blocks[order - 1].insert(BuddyBlock {
            base: 0,
            len: (base_block_cap * block_count) as u32,
        });

        Self {
            free_blocks,
            base_block_cap,
            block_count,
        }
    }

    #[inline(always)]
    pub fn base_block_cap(&self) -> usize {
        self.base_block_cap
    }

    #[inline(always)]
    pub fn block_count(&self) -> usize {
        self.block_count
    }

    /// Allocate a region of the allocator to fit `size` number of elements.
    ///
    /// Returns `None` if allocation failed, meaning the allocator must be expanded.
    pub fn allocate(&mut self, size: usize) -> Option<BuddyBlock> {
        // Must have space for the allocation
        let (level, mut upper_level) = match self.get_allocation_levels(size) {
            Some(levels) => levels,
            None => return None,
        };

        // Split the current block until we're at the desired level
        while upper_level != level {
            let block = *self.free_blocks[upper_level].iter().next().unwrap();
            self.free_blocks[upper_level].remove(&block);

            upper_level -= 1;
            let new_len = block.len / 2;

            let left_block = BuddyBlock {
                base: block.base,
                len: new_len,
            };

            let right_block = BuddyBlock {
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
    pub fn free(&mut self, mut block: BuddyBlock) {
        let mut level = ((block.len as usize / self.base_block_cap) as f32).log2() as usize;
        let mut is_even =
            ((block.base / self.base_block_cap as u32) / (1 << level as u32)) % 2 == 0;

        // If this covers the whole allocation range, no need for merging
        if level == self.free_blocks.len() {
            self.free_blocks[level].insert(block);
            return;
        }

        // Insert into free list
        self.free_blocks[level].insert(block);

        // Continue to merge until we've either hit the max level or have no more buddy
        while let Some(buddy) = self.free_blocks[level].take(&BuddyBlock {
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
            block = BuddyBlock {
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

            is_even = ((block.base / self.base_block_cap as u32) / (1 << level as u32)) % 2 == 0;
        }
    }

    /// Expands the allocator to fit a new number of blocks. Does nothing if `new_block_count` is
    /// less than the current block count.
    ///
    /// `new_block_count` must be a power of two.
    pub fn expand(&mut self, new_block_count: usize) {
        assert!(new_block_count.is_power_of_two());

        if new_block_count <= self.block_count {
            return;
        }

        let new_order = (new_block_count as f32).log2() as usize + 1;

        // If we have nothing allocated yet, just clear the current level and update the new block
        if !self.free_blocks[self.free_blocks.len() - 1].is_empty() {
            self.free_blocks.last_mut().unwrap().clear();
            self.free_blocks
                .resize(new_order, FxHashSet::<BuddyBlock>::default());
            self.free_blocks.last_mut().unwrap().insert(BuddyBlock {
                base: 0,
                len: (new_block_count * self.base_block_cap) as u32,
            });
        }
        // Things are allocated. Just add a new "right-most" block that is free to each new level
        else {
            let old_order = self.free_blocks.len() - 1;
            self.free_blocks
                .resize(new_order, FxHashSet::<BuddyBlock>::default());

            for level in old_order..(new_order - 1) {
                self.free_blocks[level].insert(BuddyBlock {
                    base: ((1 << level) * self.base_block_cap) as u32,
                    len: ((1 << level) * self.base_block_cap) as u32,
                });
            }
        }

        self.block_count = new_block_count;
    }

    /// Ensures the capacity of the allocator can accomodate the provided allocation size.
    pub fn reserve_for(&mut self, size: usize) {
        // Do nothing if we already have the capacity
        if self.has_capacity_for(size) {
            return;
        }

        // Compute what level is needed to accomodate the length
        let needed_level = (size as f32 / self.base_block_cap as f32)
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

    /// Determines if the allocator has enough capacity to fit the required allocation size.
    ///
    /// # Note
    /// `allocate` is guaranteed to succeed for the given size if this method returns true.
    pub fn has_capacity_for(&self, size: usize) -> bool {
        self.get_allocation_levels(size).is_some()
    }

    /// Gets a tuple containing the level that fits an allocation of `size` and the upper level
    /// that must be split to get to `level` in that order.
    ///
    /// If `level == upper_level`, then no split is required.
    ///
    /// If `None` is returned, then there is no capacity for the required allocation.
    fn get_allocation_levels(&self, size: usize) -> Option<(usize, usize)> {
        // Determine what level the allocation must be placed it
        let block_count =
            (size / self.base_block_cap) + usize::from(size % self.base_block_cap != 0);
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

        Some((level, upper_level))
    }
}

impl BuddyBlock {
    /// Base pointer (measured in T's) for this block.
    #[inline]
    pub fn base(&self) -> u32 {
        self.base
    }

    /// Number of T's that can fit in this block.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
