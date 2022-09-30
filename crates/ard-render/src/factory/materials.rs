use std::collections::{hash_map::Entry, HashMap, HashSet};

use ard_pal::prelude::*;

use crate::{
    material::{Material, MaterialInstance, MaterialInstanceInner},
    shader_constants::{FRAMES_IN_FLIGHT, MAX_TEXTURES_PER_MATERIAL, NO_TEXTURE},
};

use super::allocator::ResourceAllocator;

const INITIAL_MATERIAL_UBO_CAPACITY: u64 = 32;

const TEXTURE_BINDING: u32 = 0;
const MATERIAL_BINDING: u32 = 1;

pub(crate) struct MaterialBuffers {
    ctx: Context,
    layout: DescriptorSetLayout,
    buffers: HashMap<u64, MaterialBuffer>,
    /// UBO for material textures. Max number of textures is allocated per material, regardless
    /// of how many are actually used.
    texture_arrays: MaterialBuffer,
    sets: HashMap<u64, Vec<MaterialSet>>,
}

pub(crate) struct MaterialBuffer {
    data_size: u64,
    buffer: Buffer,
    dirty: [Vec<MaterialInstance>; FRAMES_IN_FLIGHT],
    capacity: u64,
    free: Vec<MaterialBlock>,
    slot_counter: usize,
    is_textures: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MaterialBlock(usize);

struct MaterialSet {
    set: DescriptorSet,
    last_buffer_size: u64,
    last_texture_size: u64,
}

impl MaterialBuffers {
    pub fn new(ctx: Context) -> Self {
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Material data
                    DescriptorBinding {
                        binding: MATERIAL_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Textures
                    DescriptorBinding {
                        binding: TEXTURE_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                ],
            },
        )
        .unwrap();

        let texture_arrays = MaterialBuffer::new(
            &ctx,
            (std::mem::size_of::<u32>() * MAX_TEXTURES_PER_MATERIAL) as u64,
            true,
        );

        Self {
            ctx,
            layout,
            texture_arrays,
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
        if let Some(buffer) = self.buffers.get_mut(&material.material.data_size) {
            buffer.mark_dirty(&material);
        }

        if material.material.texture_count > 0 {
            self.texture_arrays.mark_dirty(&material);
        }
    }

    pub fn flush(&mut self, materials: &ResourceAllocator<MaterialInstanceInner>, frame: usize) {
        // Flush textures
        let textures_resized = self.texture_arrays.flush(&self.ctx, materials, frame);

        // Flush for every buffer
        for buffer in self.buffers.values_mut() {
            let materials_resized = buffer.flush(&self.ctx, materials, frame);

            // If the material buffer was resized go for a rebind.
            if materials_resized {
                let sets = self.sets.get_mut(&buffer.data_size).unwrap();
                for (frame, set) in sets.iter_mut().enumerate() {
                    set.check_rebind_buffer(frame, buffer);
                }
            }
        }

        // If textures were resized, rebind everything
        if textures_resized {
            for sets in self.sets.values_mut() {
                for (frame, set) in sets.iter_mut().enumerate() {
                    set.check_rebind_textures(frame, &self.texture_arrays);
                }
            }
        }
    }

    #[inline(always)]
    pub fn get_set(&self, data_size: u64, frame: usize) -> Option<&DescriptorSet> {
        match self.sets.get(&data_size) {
            Some(sets) => Some(&sets[frame].set),
            None => None,
        }
    }

    pub fn get_set_mut(&mut self, data_size: u64, frame: usize) -> &DescriptorSet {
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
                    last_texture_size: 0,
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
                .or_insert_with(|| MaterialBuffer::new(&self.ctx, data_size, false));
            set[frame].check_rebind_buffer(frame, &buffer);
        }

        // Rebind textures
        set[frame].check_rebind_textures(frame, &self.texture_arrays);

        &set[frame].set
    }

    #[inline(always)]
    pub fn allocate_ubo(&mut self, data_size: u64) -> MaterialBlock {
        let buffer = self
            .buffers
            .entry(data_size)
            .or_insert_with(|| MaterialBuffer::new(&self.ctx, data_size, false));
        buffer.allocate()
    }

    #[inline(always)]
    pub fn allocate_textures(&mut self) -> MaterialBlock {
        self.texture_arrays.allocate()
    }

    #[inline(always)]
    pub fn free_ubo(&mut self, data_size: u64, block: MaterialBlock) {
        self.buffers.get_mut(&data_size).unwrap().free(block);
    }

    #[inline(always)]
    pub fn free_textures(&mut self, block: MaterialBlock) {
        self.texture_arrays.free(block)
    }
}

impl MaterialBuffer {
    fn new(context: &Context, data_size: u64, is_textures: bool) -> Self {
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
            is_textures,
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

    /// Flushes dirty material values to the buffer.
    ///
    /// Returns `true`, if the buffer was resized.
    fn flush(
        &mut self,
        ctx: &Context,
        materials: &ResourceAllocator<MaterialInstanceInner>,
        frame: usize,
    ) -> bool {
        // Resize the buffer if required
        let resized = if self.slot_counter as u64 >= self.capacity {
            let old_cap = self.capacity;
            let mut new_cap = old_cap;
            while new_cap < self.slot_counter as u64 {
                new_cap *= 2;
            }

            let new_buffer = Buffer::new(
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
                new_view[..old_view.len()].copy_from_slice(&old_view);
            }

            // Swap the old buffer with the new
            self.buffer = new_buffer;
            self.capacity = new_cap;
            true
        } else {
            false
        };

        // Flush dirty values
        let mut view = self.buffer.write(frame).unwrap();
        if self.is_textures {
            for dirty_mat in self.dirty[frame].drain(..) {
                let material = materials.get(dirty_mat.id).unwrap();
                let offset = material.texture_block.unwrap().0 * self.data_size as usize;
                for (i, texture) in material.textures.iter().enumerate() {
                    let offset = offset + (i * std::mem::size_of::<u32>());
                    let rng = offset..(offset + std::mem::size_of::<u32>());
                    let id = match texture {
                        Some(tex) => tex.id.0 as u32,
                        None => NO_TEXTURE,
                    };
                    view[rng].copy_from_slice(bytemuck::bytes_of(&id));
                }
            }
        } else {
            for dirty_mat in self.dirty[frame].drain(..) {
                let material = materials.get(dirty_mat.id).unwrap();
                let offset = material.material_block.unwrap().0 * self.data_size as usize;
                let rng = offset..(offset + self.data_size as usize);
                view[rng].copy_from_slice(&material.data);
            }
        }

        resized
    }
}

impl MaterialSet {
    pub fn check_rebind_textures(&mut self, frame: usize, texture_buffer: &MaterialBuffer) {
        if self.last_texture_size != texture_buffer.buffer.size() {
            self.set.update(&[DescriptorSetUpdate {
                binding: TEXTURE_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &texture_buffer.buffer,
                    array_element: frame,
                },
            }]);
            self.last_texture_size = texture_buffer.buffer.size();
        }
    }

    pub fn check_rebind_buffer(&mut self, frame: usize, material_buffer: &MaterialBuffer) {
        if self.last_buffer_size != material_buffer.buffer.size() {
            self.set.update(&[DescriptorSetUpdate {
                binding: MATERIAL_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &material_buffer.buffer,
                    array_element: frame,
                },
            }]);
            self.last_buffer_size = material_buffer.buffer.size();
        }
    }
}

impl From<MaterialBlock> for usize {
    #[inline(always)]
    fn from(block: MaterialBlock) -> Self {
        block.0
    }
}

impl From<MaterialBlock> for u32 {
    #[inline(always)]
    fn from(block: MaterialBlock) -> Self {
        block.0 as u32
    }
}
