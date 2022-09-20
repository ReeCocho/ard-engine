use std::collections::{HashMap, HashSet};

use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;

use crate::mesh::{AttributeType, VertexLayout};

const DEFAULT_VB_LEN: usize = 1;
const DEFAULT_IB_LEN: usize = 1;
const BASE_VERTEX_BLOCK_LEN: usize = 64;
const BASE_INDEX_BLOCK_LEN: usize = 256;

pub(crate) struct MeshBuffers {
    ctx: Context,
    vertex_buffers: HashMap<VertexLayout, VertexBuffers>,
    index_buffer: BufferArrayAllocator,
}

pub(crate) struct VertexBuffers {
    layout: VertexLayout,
    /// Holds the buffer and which attribute the buffer contains.
    buffers: Vec<(BufferArrayAllocator, AttributeType)>,
}

pub(crate) struct BufferArrayAllocator {
    buffer: Buffer,
    free_blocks: Vec<HashSet<MeshBlock>>,
    /// Number of Ts that can fit in the smallest block size.
    base_block_len: usize,
    /// Total number of base blocks. Must be a power of 2.
    block_count: usize,
    /// Size of objects allocated.
    object_size: usize,
}

#[derive(Copy, Clone, Eq)]
pub(crate) struct MeshBlock {
    base: u32,
    len: u32,
}

impl MeshBuffers {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx: ctx.clone(),
            vertex_buffers: HashMap::default(),
            index_buffer: BufferArrayAllocator::new(
                ctx.clone(),
                BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST | BufferUsage::TRANSFER_SRC,
                BASE_INDEX_BLOCK_LEN,
                DEFAULT_IB_LEN,
                std::mem::size_of::<u32>(),
            ),
        }
    }

    #[inline(always)]
    pub fn get_index_buffer(&self) -> &BufferArrayAllocator {
        &self.index_buffer
    }

    #[inline(always)]
    pub fn get_index_buffer_mut(&mut self) -> &mut BufferArrayAllocator {
        &mut self.index_buffer
    }

    #[inline(always)]
    pub fn get_vertex_buffer(&self, layout: VertexLayout) -> Option<&VertexBuffers> {
        self.vertex_buffers.get(&layout)
    }

    #[inline(always)]
    pub fn get_vertex_buffer_mut(&mut self, layout: VertexLayout) -> &mut VertexBuffers {
        self.vertex_buffers
            .entry(layout)
            .or_insert_with(|| VertexBuffers::new(&self.ctx, layout, DEFAULT_VB_LEN))
    }
}

impl VertexBuffers {
    fn new(ctx: &Context, layout: VertexLayout, block_count: usize) -> Self {
        let mut buffers = vec![(
            BufferArrayAllocator::new(
                ctx.clone(),
                BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
                BASE_VERTEX_BLOCK_LEN,
                block_count,
                std::mem::size_of::<Vec4>(),
            ),
            AttributeType::Position,
        )];

        if layout.normals {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec4>(),
                ),
                AttributeType::Normal,
            ));
        }

        if layout.tangents {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec4>(),
                ),
                AttributeType::Tangent,
            ));
        }

        if layout.colors {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec4>(),
                ),
                AttributeType::Color,
            ));
        }

        if layout.uv0 {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec2>(),
                ),
                AttributeType::Uv0,
            ));
        }

        if layout.uv1 {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec2>(),
                ),
                AttributeType::Uv1,
            ));
        }

        if layout.uv2 {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec2>(),
                ),
                AttributeType::Uv2,
            ));
        }

        if layout.uv3 {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx.clone(),
                    BufferUsage::VERTEX_BUFFER
                        | BufferUsage::TRANSFER_SRC
                        | BufferUsage::TRANSFER_DST,
                    BASE_VERTEX_BLOCK_LEN,
                    block_count,
                    std::mem::size_of::<Vec2>(),
                ),
                AttributeType::Uv3,
            ));
        }

        Self { buffers, layout }
    }

    /// Binds the internal buffers for the target layout, assuming it is a subset of our attributes.
    pub fn bind<'a>(&'a self, render_pass: &mut RenderPass<'a>, target_layout: VertexLayout) {
        assert!(target_layout.subset_of(&self.layout));

        let mut binds = Vec::default();
        for (container, id) in &self.buffers {
            match *id {
                AttributeType::Position => {}
                AttributeType::Normal => {
                    if !target_layout.normals {
                        continue;
                    }
                }
                AttributeType::Tangent => {
                    if !target_layout.tangents {
                        continue;
                    }
                }
                AttributeType::Color => {
                    if !target_layout.colors {
                        continue;
                    }
                }
                AttributeType::Uv0 => {
                    if !target_layout.uv0 {
                        continue;
                    }
                }
                AttributeType::Uv1 => {
                    if !target_layout.uv1 {
                        continue;
                    }
                }
                AttributeType::Uv2 => {
                    if !target_layout.uv2 {
                        continue;
                    }
                }
                AttributeType::Uv3 => {
                    if !target_layout.uv3 {
                        continue;
                    }
                }
            }

            binds.push(VertexBind {
                buffer: container.buffer(),
                array_element: 0,
                offset: 0,
            });
        }

        render_pass.bind_vertex_buffers(0, binds);
    }

    #[inline(always)]
    pub fn buffer(&self, idx: usize) -> &Buffer {
        self.buffers[idx].0.buffer()
    }

    #[inline]
    pub fn allocate(&mut self, count: usize) -> Option<MeshBlock> {
        // Buffer 0 is the position buffer which always exists. Since the state of all allocators
        // is the same, if this fails all other ones will also fail and need expansion. If it
        // succeeds, then all allocated blocks will be the same.
        if let Some(block) = self.buffers[0].0.allocate(count) {
            for buffer in &mut self.buffers[1..] {
                buffer.0.allocate(count);
            }

            Some(block)
        } else {
            None
        }
    }

    #[inline]
    pub fn free(&mut self, block: MeshBlock) {
        for buffer in &mut self.buffers {
            buffer.0.free(block);
        }
    }

    /// Given a number of vertices to allocate, creates a new level such that the newest max level
    /// can fit all the vertices.
    pub fn expand_for(&mut self, ctx: &Context, vertex_count: usize) {
        for buffer in &mut self.buffers {
            buffer.0.expand_for(ctx, vertex_count);
        }
    }
}

impl BufferArrayAllocator {
    pub fn new(
        ctx: Context,
        usage: BufferUsage,
        // How many objects can be held in the lowest level block.
        base_block_len: usize,
        // The number of lowest level blocks that can be allocated. Must be a power of two.
        block_count: usize,
        // Size of the objects to allocate.
        object_size: usize,
    ) -> Self {
        assert!(block_count.is_power_of_two());

        let order = (block_count as f32).log2() as usize + 1;
        let mut free_blocks = Vec::with_capacity(order);
        free_blocks.resize(order, HashSet::<MeshBlock, _>::default());
        free_blocks[order - 1].insert(MeshBlock {
            base: 0,
            len: (base_block_len * block_count) as u32,
        });

        let buffer = Buffer::new(
            ctx,
            BufferCreateInfo {
                size: (object_size * base_block_len * block_count) as u64,
                array_elements: 1,
                buffer_usage: usage,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: None,
            },
        )
        .unwrap();

        Self {
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

    pub fn allocate(&mut self, count: usize) -> Option<MeshBlock> {
        // Determine what level the allocation must be placed it
        let block_count = (count / self.base_block_len)
            + if count % self.base_block_len != 0 {
                1
            } else {
                0
            };
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

            let left_block = MeshBlock {
                base: block.base,
                len: new_len,
            };

            let right_block = MeshBlock {
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

    pub fn free(&mut self, mut block: MeshBlock) {
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
        while let Some(buddy) = self.free_blocks[level].take(&MeshBlock {
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
            block = MeshBlock {
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

    /// Resizes the buffer to fit a new number of blocks. Records a copy from the old buffer to the
    /// new buffer.
    pub fn resize(&mut self, ctx: &Context, new_block_count: usize) {
        assert!(new_block_count.is_power_of_two());

        if new_block_count < self.block_count {
            return;
        }

        let new_order = (new_block_count as f32).log2() as usize + 1;

        // If we have nothing allocated yet, just clear the current level and update the new block
        if !self.free_blocks[self.free_blocks.len() - 1].is_empty() {
            self.free_blocks.last_mut().unwrap().clear();
            self.free_blocks
                .resize(new_order, HashSet::<MeshBlock, _>::default());
            self.free_blocks.last_mut().unwrap().insert(MeshBlock {
                base: 0,
                len: (new_block_count * self.base_block_len) as u32,
            });
        }
        // Things are allocated. Just add a new "right-most" block that is free to each new level
        else {
            let old_order = self.free_blocks.len() - 1;
            self.free_blocks
                .resize(new_order, HashSet::<MeshBlock, _>::default());

            for level in old_order..(new_order - 1) {
                self.free_blocks[level].insert(MeshBlock {
                    base: ((1 << level) * self.base_block_len) as u32,
                    len: ((1 << level) * self.base_block_len) as u32,
                });
            }
        }

        self.block_count = new_block_count;

        // Create new buffer
        let new_buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (self.object_size * self.base_block_len * self.block_count) as u64,
                array_elements: 1,
                buffer_usage: self.buffer.buffer_usage(),
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: None,
            },
        )
        .unwrap();

        // Record copy command
        ctx.transfer()
            .submit(Some("resize_vertex_buffer"), |commands| {
                commands.copy_buffer_to_buffer(CopyBufferToBuffer {
                    src: &self.buffer,
                    src_array_element: 0,
                    src_offset: 0,
                    dst: &new_buffer,
                    dst_array_element: 0,
                    dst_offset: 0,
                    len: self.buffer.size(),
                });
            });

        // Swap old and new buffer
        self.buffer = new_buffer;
    }

    /// Expands the buffer such that it can fit the requested array length.
    pub fn expand_for(&mut self, ctx: &Context, count: usize) {
        // Compute what level is needed to accomodate the length
        let needed_level = (count as f32 / self.base_block_len as f32)
            .ceil()
            .log2()
            .ceil() as usize;

        // If our needed level is smaller than the current max level, all we need is one more level
        if needed_level < self.free_blocks.len() {
            self.resize(ctx, self.block_count * 2);
        }
        // If our needed level is larger than the current max level, we need to expand to however
        // many levels is requested by needed_level
        else {
            self.resize(ctx, 1 << (needed_level + 1));
        }
    }
}

impl MeshBlock {
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
}

impl PartialEq for MeshBlock {
    /// No two blocks can have the same base, so this is sufficient.
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
    }
}

impl std::hash::Hash for MeshBlock {
    /// No two blocks can have the same base, so this is sufficient.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.base);
    }
}
