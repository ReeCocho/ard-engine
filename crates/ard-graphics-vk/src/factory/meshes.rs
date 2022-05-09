use glam::{Vec2, Vec4};
use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasherDefault, Hash},
};

use crate::prelude::*;
use ash::vk;
use gpu_alloc::UsageFlags;

use crate::{
    alloc::{Buffer, BufferCreateInfo},
    context::GraphicsContext,
    util::FastIntHasher,
};

const BASE_VERTEX_BLOCK_LEN: usize = 64;
const BASE_INDEX_BLOCK_LEN: usize = 256;

#[derive(Copy, Clone)]
enum AttributeType {
    Position,
    Normal,
    Tangent,
    Color,
    Uv0,
    Uv1,
    Uv2,
    Uv3,
}

pub(crate) struct MeshBuffers {
    ctx: GraphicsContext,
    default_vb_len: usize,
    vertex_buffers: HashMap<VertexLayout, VertexBuffers>,
    index_buffer: BufferArrayAllocator,
}

/// Contains the vertex buffers that match a particular vertex layout.
pub(crate) struct VertexBuffers {
    layout: VertexLayout,
    /// Holds the buffer and which attribute the buffer contains.
    buffers: Vec<(BufferArrayAllocator, AttributeType)>,
}

pub(crate) struct BufferArrayAllocator {
    ctx: GraphicsContext,
    /// GPU buffer containing our objects.
    buffer: Buffer,
    free_blocks: Vec<HashSet<Block, BuildHasherDefault<FastIntHasher>>>,
    /// Number of Ts that can fit in the smallest block size.
    base_block_len: usize,
    /// Total number of base blocks. Must be a power of 2.
    block_count: usize,
    object_size: usize,
}

#[derive(Copy, Clone, Eq)]
pub(crate) struct Block {
    base: u32,
    len: u32,
}

impl MeshBuffers {
    pub unsafe fn new(ctx: &GraphicsContext, default_vb_len: usize, default_ib_len: usize) -> Self {
        assert!(default_vb_len.is_power_of_two());
        assert!(default_ib_len.is_power_of_two());

        Self {
            ctx: ctx.clone(),
            vertex_buffers: HashMap::default(),
            default_vb_len,
            index_buffer: BufferArrayAllocator::new(
                ctx,
                vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC,
                BASE_INDEX_BLOCK_LEN,
                default_ib_len,
                std::mem::size_of::<u32>(),
            ),
        }
    }

    pub unsafe fn get_index_buffer(&mut self) -> &mut BufferArrayAllocator {
        &mut self.index_buffer
    }

    pub unsafe fn get_vertex_buffer(&mut self, layout: &VertexLayout) -> &mut VertexBuffers {
        // TODO: When Rust 2021 releases we won't need to use temporaries for this borrow in the closure
        let ctx = &self.ctx;
        let default_vb_len = self.default_vb_len;
        self.vertex_buffers
            .entry(*layout)
            .or_insert_with(|| VertexBuffers::new(ctx, *layout, default_vb_len))
    }
}

impl VertexBuffers {
    unsafe fn new(ctx: &GraphicsContext, layout: VertexLayout, block_count: usize) -> Self {
        let mut buffers = vec![(
            BufferArrayAllocator::new(
                ctx,
                vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
                BASE_VERTEX_BLOCK_LEN,
                block_count,
                std::mem::size_of::<Vec4>(),
            ),
            AttributeType::Position,
        )];

        if layout.normals {
            buffers.push((
                BufferArrayAllocator::new(
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
                    ctx,
                    vk::BufferUsageFlags::VERTEX_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
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
    pub unsafe fn bind(
        &self,
        device: &ash::Device,
        commands: vk::CommandBuffer,
        target_layout: &VertexLayout,
    ) {
        assert!(target_layout.subset_of(&self.layout));

        let mut cur_attribute = 0;
        let offsets = [0; MAX_VERTEX_ATTRIBUTE_COUNT];
        let mut buffers = [vk::Buffer::null(); MAX_VERTEX_ATTRIBUTE_COUNT];

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

            buffers[cur_attribute] = container.buffer();
            cur_attribute += 1;
        }

        device.cmd_bind_vertex_buffers(
            commands,
            0,
            &buffers[0..cur_attribute],
            &offsets[0..cur_attribute],
        );
    }

    pub fn buffer(&mut self, idx: usize) -> vk::Buffer {
        self.buffers[idx].0.buffer()
    }

    pub fn allocate(&mut self, count: usize) -> Option<Block> {
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

    pub fn free(&mut self, block: Block) {
        for buffer in &mut self.buffers {
            buffer.0.free(block);
        }
    }

    /// Given a number of vertices to allocate, creates a new level such that the newest max level
    /// can fit all the vertices.
    pub unsafe fn expand_for(
        &mut self,
        commands: vk::CommandBuffer,
        vertex_count: usize,
    ) -> Vec<Buffer> {
        let mut buffers = Vec::with_capacity(self.buffers.len());

        for buffer in &mut self.buffers {
            if let Some(buffer) = buffer.0.expand_for(vertex_count, commands) {
                buffers.push(buffer)
            }
        }

        buffers
    }
}

impl BufferArrayAllocator {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        usage: vk::BufferUsageFlags,
        base_block_len: usize,
        block_count: usize,
        object_size: usize,
    ) -> Self {
        assert!(block_count.is_power_of_two());

        let order = (block_count as f32).log2() as usize + 1;
        let mut free_blocks = Vec::with_capacity(order);
        free_blocks.resize(
            order,
            HashSet::<Block, BuildHasherDefault<FastIntHasher>>::default(),
        );
        free_blocks[order - 1].insert(Block {
            base: 0,
            len: (base_block_len * block_count) as u32,
        });

        // Create buffer
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size: (object_size * base_block_len * block_count) as u64,
            buffer_usage: usage,
            memory_usage: UsageFlags::FAST_DEVICE_ACCESS,
        };

        let buffer = Buffer::new(&create_info);

        Self {
            ctx: ctx.clone(),
            buffer,
            block_count,
            base_block_len,
            free_blocks,
            object_size,
        }
    }

    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer()
    }

    pub fn allocate(&mut self, count: usize) -> Option<Block> {
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

            let left_block = Block {
                base: block.base,
                len: new_len,
            };

            let right_block = Block {
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

    pub fn free(&mut self, mut block: Block) {
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
        while let Some(buddy) = self.free_blocks[level].take(&Block {
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
            block = Block {
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
    /// new buffer using `commands.` Returns the old buffer so it can be held in memory until the
    /// copy is complete.
    pub unsafe fn resize(
        &mut self,
        new_block_count: usize,
        commands: vk::CommandBuffer,
    ) -> Option<Buffer> {
        assert!(new_block_count.is_power_of_two());

        if new_block_count < self.block_count {
            return None;
        }

        let new_order = (new_block_count as f32).log2() as usize + 1;

        // If we have nothing allocated yet, just clear the current level and update the new block
        if !self.free_blocks[self.free_blocks.len() - 1].is_empty() {
            self.free_blocks.last_mut().unwrap().clear();
            self.free_blocks.resize(
                new_order,
                HashSet::<Block, BuildHasherDefault<FastIntHasher>>::default(),
            );
            self.free_blocks.last_mut().unwrap().insert(Block {
                base: 0,
                len: (new_block_count * self.base_block_len) as u32,
            });
        }
        // Things are allocated. Just add a new "right-most" block that is free to each new level
        else {
            let old_order = self.free_blocks.len() - 1;
            self.free_blocks.resize(
                new_order,
                HashSet::<Block, BuildHasherDefault<FastIntHasher>>::default(),
            );

            for level in old_order..(new_order - 1) {
                self.free_blocks[level].insert(Block {
                    base: ((1 << level) * self.base_block_len) as u32,
                    len: ((1 << level) * self.base_block_len) as u32,
                });
            }
        }

        self.block_count = new_block_count;

        // Create new buffer
        let create_info = BufferCreateInfo {
            ctx: self.ctx.clone(),
            size: (self.object_size * self.base_block_len * self.block_count) as u64,
            buffer_usage: self.buffer.usage(),
            memory_usage: UsageFlags::FAST_DEVICE_ACCESS,
        };

        let mut new_buffer = Buffer::new(&create_info);

        // Record copy command
        let regions = [vk::BufferCopy::builder()
            .dst_offset(0)
            .src_offset(0)
            .size(self.buffer.size())
            .build()];

        self.ctx.0.device.cmd_copy_buffer(
            commands,
            self.buffer.buffer(),
            new_buffer.buffer,
            &regions,
        );

        // Swap old and new buffer and return the old buffer
        std::mem::swap(&mut self.buffer, &mut new_buffer);
        Some(new_buffer)
    }

    /// Expands the buffer such that it can fit the requested array length.
    pub unsafe fn expand_for(
        &mut self,
        count: usize,
        commands: vk::CommandBuffer,
    ) -> Option<Buffer> {
        // Compute what level is needed to accomodate the length
        let needed_level = (count as f32 / self.base_block_len as f32)
            .ceil()
            .log2()
            .ceil() as usize;

        // If our needed level is smaller than the current max level, all we need is one more level
        if needed_level < self.free_blocks.len() {
            self.resize(self.block_count * 2, commands)
        }
        // If our needed level is larger than the current max level, we need to expand to however
        // many levels is requested by needed_level
        else {
            self.resize(1 << (needed_level + 1), commands)
        }
    }
}

impl Block {
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

impl PartialEq for Block {
    /// No two blocks can have the same base, so this is sufficient.
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
    }
}

impl Hash for Block {
    /// No two blocks can have the same base, so this is sufficient.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.base);
    }
}
