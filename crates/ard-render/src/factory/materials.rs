use std::collections::{hash_map::Entry, HashMap};

use ard_pal::prelude::*;

use crate::{
    material::{Material, MaterialInstance, MaterialInstanceInner},
    renderer::FRAMES_IN_FLIGHT,
};

use super::allocator::ResourceAllocator;

const INITIAL_MATERIAL_UBO_CAPACITY: u64 = 32;

const MATERIAL_BINDING: u32 = 0;
const TEXTURE_BINDING: u32 = 1;

pub(crate) struct MaterialBuffers {
    ctx: Context,
    layout: DescriptorSetLayout,
    buffers: HashMap<u64, MaterialBuffer>,
    sets: HashMap<u64, Vec<MaterialSet>>,
}

pub(crate) struct MaterialBuffer {
    data_size: u64,
    buffer: Buffer,
    dirty: [Vec<MaterialInstance>; FRAMES_IN_FLIGHT],
    capacity: u64,
    free: Vec<MaterialBlock>,
    slot_counter: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MaterialBlock(usize);

struct MaterialSet {
    set: DescriptorSet,
    last_buffer_size: u64,
}

impl MaterialBuffers {
    pub fn new(ctx: Context) -> Self {
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![DescriptorBinding {
                    binding: MATERIAL_BINDING,
                    ty: DescriptorType::StorageBuffer(AccessType::Read),
                    count: 1,
                    stage: ShaderStage::AllGraphics,
                }],
            },
        )
        .unwrap();

        Self {
            ctx,
            layout,
            buffers: HashMap::default(),
            sets: HashMap::default(),
        }
    }

    #[inline(always)]
    pub fn layout(&self) -> &DescriptorSetLayout {
        &self.layout
    }

    #[inline(always)]
    pub fn mark_dirty(&mut self, material: MaterialInstance) {
        self.buffers
            .get_mut(&material.material.data_size)
            .unwrap()
            .mark_dirty(&material);
    }

    pub fn flush(&mut self, materials: &ResourceAllocator<MaterialInstanceInner>, frame: usize) {
        for buffer in self.buffers.values_mut() {
            buffer.flush(&self.ctx, materials, frame);
        }
    }

    pub fn get_set(&mut self, data_size: u64, frame: usize) -> &DescriptorSet {
        // If the set doesn't already exist, create it
        if let Entry::Vacant(entry) = self.sets.entry(data_size) {
            let mut sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
            for i in 0..FRAMES_IN_FLIGHT {
                sets.push(MaterialSet {
                    set: DescriptorSet::new(
                        self.ctx.clone(),
                        DescriptorSetCreateInfo {
                            layout: self.layout.clone(),
                            debug_name: Some(format!("material_set_{}_frame_{}", data_size, i)),
                        },
                    )
                    .unwrap(),
                    last_buffer_size: 0,
                });
            }
            entry.insert(sets);
        }

        let set = self.sets.get_mut(&data_size).unwrap();

        // Rebind UBO if needed
        if data_size != 0 {
            let buffer = self
                .buffers
                .entry(data_size)
                .or_insert_with(|| MaterialBuffer::new(&self.ctx, data_size));
            if set[frame].last_buffer_size != buffer.buffer.size() {
                set[frame].last_buffer_size = buffer.buffer.size();
                set[frame].set.update(&[DescriptorSetUpdate {
                    binding: MATERIAL_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &buffer.buffer,
                        array_element: frame,
                    },
                }]);
            }
        }

        &set[frame].set
    }

    #[inline(always)]
    pub fn allocate_ubo(&mut self, data_size: u64) -> MaterialBlock {
        let buffer = self
            .buffers
            .entry(data_size)
            .or_insert_with(|| MaterialBuffer::new(&self.ctx, data_size));
        buffer.allocate()
    }

    #[inline(always)]
    pub fn free_ubo(&mut self, data_size: u64, block: MaterialBlock) {
        self.buffers.get_mut(&data_size).unwrap().free(block);
    }
}

impl MaterialBuffer {
    fn new(context: &Context, data_size: u64) -> Self {
        let buffer = Buffer::new(
            context.clone(),
            BufferCreateInfo {
                size: data_size * INITIAL_MATERIAL_UBO_CAPACITY,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(format!("material_buffer_{}", data_size)),
            },
        )
        .unwrap();

        MaterialBuffer {
            data_size,
            buffer,
            dirty: Default::default(),
            capacity: INITIAL_MATERIAL_UBO_CAPACITY,
            free: Vec::default(),
            slot_counter: 0,
        }
    }

    #[inline(always)]
    fn allocate(&mut self) -> MaterialBlock {
        match self.free.pop() {
            Some(block) => block,
            None => {
                self.slot_counter += 1;
                MaterialBlock(self.slot_counter - 1)
            }
        }
    }

    #[inline(always)]
    fn free(&mut self, block: MaterialBlock) {
        self.free.push(block);
    }

    #[inline(always)]
    fn mark_dirty(&mut self, material: &MaterialInstance) {
        for dirty in &mut self.dirty {
            dirty.push(material.clone());
        }
    }

    fn flush(
        &mut self,
        ctx: &Context,
        materials: &ResourceAllocator<MaterialInstanceInner>,
        frame: usize,
    ) {
        // Resize the buffer if required
        if self.slot_counter as u64 >= self.capacity {
            let old_cap = self.capacity;
            let mut new_cap = old_cap;
            while new_cap < self.slot_counter as u64 {
                new_cap *= 2;
            }

            let mut new_buffer = Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: self.data_size * new_cap,
                    array_elements: FRAMES_IN_FLIGHT,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    debug_name: Some(format!("material_buffer_{}", self.data_size)),
                },
            )
            .unwrap();

            // Copy data from the old buffer into the new buffer
            for frame in 0..FRAMES_IN_FLIGHT {
                let old_view = self.buffer.read(frame).unwrap();
                let mut new_view = new_buffer.write(frame).unwrap();
                new_view.as_slice_mut().copy_from_slice(old_view.as_slice());
            }

            // Swap the old buffer with the new
            self.buffer = new_buffer;
            self.capacity = new_cap;
        }

        // Flush dirty values
        let mut view = self.buffer.write(frame).unwrap();
        let slice = view.as_slice_mut();
        for dirty_mat in self.dirty[frame].drain(..) {
            let material = materials.get(dirty_mat.id).unwrap();
            let offset = material.material_block.unwrap().0 * self.data_size as usize;
            slice[offset..].copy_from_slice(&material.data);
        }
    }
}
