use ash::vk::{self, BufferUsageFlags};
use gpu_alloc::UsageFlags;
use renderer::graph::FRAMES_IN_FLIGHT;
use std::{collections::HashMap, hash::BuildHasherDefault, ptr::NonNull};

use crate::{
    alloc::{Buffer, BufferCreateInfo},
    prelude::*,
    util::FastIntHasher,
};

use super::{container::ResourceContainer, descriptors::DescriptorPool};

const INITIAL_MATERIAL_UBO_CAPACITY: u64 = 32;

const MATERIAL_SETS_PER_POOL: usize = 32;

pub(crate) struct MaterialBuffers {
    ctx: GraphicsContext,
    pool: DescriptorPool,
    /// Maps the size of an object to a tightly packed UBO of objects of the same size.
    buffers: HashMap<u64, MaterialBuffer, BuildHasherDefault<FastIntHasher>>,
    /// UBO for material textures. Max number of textures is allocated per material, regardless
    /// of how many are actually used.
    texture_arrays: MaterialBuffer,
    /// Descriptor sets for material buffers, keyed by the size of the material data.
    sets: HashMap<u64, [MaterialSet; FRAMES_IN_FLIGHT], BuildHasherDefault<FastIntHasher>>,
}

pub(crate) struct MaterialBuffer {
    /// Size of data held within the buffer.
    data_size: u64,
    /// One material buffer per frame in flight.
    buffers: Vec<(Buffer, NonNull<u8>)>,
    /// Marks certain materials within the buffer as needing a flush to the GPU.
    dirty: [Vec<Material>; FRAMES_IN_FLIGHT],
    /// Free slots within the buffers.
    free: Vec<u32>,
    /// ID counter for slots within the buffers.
    slot_counter: u32,
}

/// Descriptor set that combines the textures array with a material data buffer.
#[derive(Default, Debug, Copy, Clone)]
pub(crate) struct MaterialSet {
    pub set: vk::DescriptorSet,
    last_buffer_size: u64,
    last_texture_size: u64,
}

unsafe impl Send for MaterialBuffer {}
unsafe impl Sync for MaterialBuffer {}

impl MaterialBuffers {
    pub unsafe fn new(ctx: &GraphicsContext) -> Self {
        let pool = {
            let bindings = [
                // Texture arrays
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
                // Material data
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                    .build(),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, MATERIAL_SETS_PER_POOL)
        };

        let texture_arrays = MaterialBuffer::new(
            ctx,
            (std::mem::size_of::<u32>() * VkBackend::MAX_TEXTURES_PER_MATERIAL) as u64,
        );

        Self {
            ctx: ctx.clone(),
            pool,
            buffers: HashMap::default(),
            sets: HashMap::default(),
            texture_arrays,
        }
    }

    pub unsafe fn flush(&mut self, materials: &ResourceContainer<MaterialInner>, frame: usize) {
        for (_, buffer) in self.buffers.iter_mut() {
            buffer.flush(&self.ctx, materials, frame, false);
        }

        self.texture_arrays.flush(&self.ctx, materials, frame, true);
    }

    #[inline]
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.pool.layout()
    }

    #[inline]
    pub fn texture_arrays(&self) -> &MaterialBuffer {
        &self.texture_arrays
    }

    #[inline]
    pub fn texture_arrays_mut(&mut self) -> &mut MaterialBuffer {
        &mut self.texture_arrays
    }

    #[inline]
    pub fn buffer(&self, data_size: u64) -> Option<&MaterialBuffer> {
        self.buffers.get(&data_size)
    }

    #[inline]
    pub fn buffer_mut(&mut self, data_size: u64) -> Option<&mut MaterialBuffer> {
        self.buffers.get_mut(&data_size)
    }

    #[inline]
    pub unsafe fn allocate_ubo(&mut self, data_size: u64) -> u32 {
        let ctx = &self.ctx;
        let buffer = self
            .buffers
            .entry(data_size)
            .or_insert_with(|| MaterialBuffer::new(ctx, data_size));
        buffer.allocate()
    }

    #[inline]
    pub fn free_ubo(&mut self, data_size: u64, idx: u32) {
        self.buffers
            .get_mut(&data_size)
            .expect("invalid buffer")
            .free(idx);
    }

    #[inline]
    pub unsafe fn allocate_textures(&mut self) -> u32 {
        self.texture_arrays.allocate()
    }

    #[inline]
    pub fn free_textures(&mut self, idx: u32) {
        self.texture_arrays.free(idx)
    }

    pub fn get_set(&mut self, data_size: u64, frame: usize) -> &MaterialSet {
        // If the buffer doesn't already exist, make one
        let ctx = &self.ctx;
        let textures = &self.texture_arrays.buffers;

        // Create set if it doesn't exist already
        if let std::collections::hash_map::Entry::Vacant(e) = self.sets.entry(data_size) {
            // Allocate and update sets
            let mut sets = [MaterialSet::default(); FRAMES_IN_FLIGHT];
            for set in sets.iter_mut() {
                set.set = unsafe { self.pool.allocate() };
                set.last_texture_size = 0;
                set.last_buffer_size = 0;
            }

            // Create material set
            e.insert(sets);
        }

        // Set is guaranteed to exist at this point
        let set = self.sets.get_mut(&data_size).unwrap();

        // Check if texture size is appropriate
        if set[frame].last_texture_size != textures[frame].0.size() {
            let buffer_info = [vk::DescriptorBufferInfo::builder()
                .buffer(textures[frame].0.buffer())
                .offset(0)
                .range(textures[frame].0.size())
                .build()];

            let write = [vk::WriteDescriptorSet::builder()
                .buffer_info(&buffer_info)
                .dst_set(set[frame].set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .build()];

            unsafe {
                self.ctx.0.device.update_descriptor_sets(&write, &[]);
            }
        }

        // Check if material data size is appropriate
        if data_size != 0 {
            let buffer = self
                .buffers
                .entry(data_size)
                .or_insert_with(|| unsafe { MaterialBuffer::new(ctx, data_size) });

            if set[frame].last_buffer_size != buffer.buffers[frame].0.size() {
                let buffer_info = [vk::DescriptorBufferInfo::builder()
                    .buffer(buffer.buffers[frame].0.buffer())
                    .offset(0)
                    .range(buffer.buffers[frame].0.size())
                    .build()];

                let write = [vk::WriteDescriptorSet::builder()
                    .buffer_info(&buffer_info)
                    .dst_set(set[frame].set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .build()];

                unsafe {
                    self.ctx.0.device.update_descriptor_sets(&write, &[]);
                }
            }
        }

        &set[frame]
    }
}

impl MaterialBuffer {
    unsafe fn new(ctx: &GraphicsContext, data_size: u64) -> Self {
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            buffer_usage: vk::BufferUsageFlags::STORAGE_BUFFER,
            memory_usage: UsageFlags::UPLOAD,
            size: INITIAL_MATERIAL_UBO_CAPACITY * data_size,
        };

        let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            let mut buffer = Buffer::new(&create_info);
            let map = buffer.map(&ctx.0.device);
            buffers.push((buffer, map));
        }

        Self {
            data_size,
            buffers,
            dirty: Default::default(),
            free: Vec::default(),
            slot_counter: 0,
        }
    }

    #[inline]
    fn allocate(&mut self) -> u32 {
        let id = if let Some(id) = self.free.pop() {
            id
        } else {
            self.slot_counter
        };

        self.slot_counter += 1;
        id
    }

    #[inline]
    fn free(&mut self, idx: u32) {
        self.free.push(idx);
    }

    #[inline]
    pub fn mark_dirty(&mut self, material: &Material) {
        for dirty in &mut self.dirty {
            dirty.push(material.clone())
        }
    }

    /// If `is_textures` is `true`, then the textures array within the material will be read.
    ///
    /// # Safety
    /// External syncronization is required to ensure that if the internal buffer is resized that
    /// it isn't in use.
    unsafe fn flush(
        &mut self,
        ctx: &GraphicsContext,
        materials: &ResourceContainer<MaterialInner>,
        frame: usize,
        is_textures: bool,
    ) {
        let device = &ctx.0.device;
        let atom_mask = ctx
            .0
            .properties
            .limits
            .non_coherent_atom_size
            .saturating_sub(1);
        let (buffer, map) = &mut self.buffers[frame];

        // Resize buffer if needed
        let cap = buffer.size() / self.data_size;
        if buffer.size() / self.data_size < self.slot_counter as u64 {
            let mut new_cap = cap;
            while new_cap < self.slot_counter as u64 {
                new_cap *= 2;
            }

            let create_info = BufferCreateInfo {
                ctx: ctx.clone(),
                size: new_cap * self.data_size,
                buffer_usage: BufferUsageFlags::STORAGE_BUFFER,
                memory_usage: UsageFlags::UPLOAD,
            };

            let mut new_buffer = Buffer::new(&create_info);
            let mut new_map = new_buffer.map(device);

            // Copy contents from the original buffer into the new buffer and flush
            std::ptr::copy_nonoverlapping(map.as_ptr(), new_map.as_ptr(), buffer.size() as usize);
            new_buffer.flush(device, 0, buffer.size());

            // Swap new buffer and old buffer
            std::mem::swap(buffer, &mut new_buffer);
            std::mem::swap(map, &mut new_map);

            // New buffer now contains the old buffer. Unmap and let drop
            new_buffer.unmap(device);
        }

        // TODO: This is gross. Maybe do something different?
        if is_textures {
            for dirty_mat in self.dirty[frame].drain(..) {
                let material = materials.get(dirty_mat.id).expect("invalid material");
                let idx = material
                    .texture_slot
                    .expect("attempt to update texutre on material without textures");
                let offset = idx as usize * self.data_size as usize;
                let dst = map.as_ptr().add(offset) as *mut u32;

                for (i, texture) in material.textures.iter().enumerate() {
                    *dst.add(i) = match texture {
                        Some(texture) => texture.id,
                        None => VkBackend::MAX_TEXTURES as u32 - 1,
                    };
                }

                let aligned_start = align_down(offset as u64, atom_mask);
                let aligned_end = align_up(offset as u64 + self.data_size, atom_mask);
                buffer.flush(
                    device,
                    align_down(aligned_start, atom_mask),
                    aligned_end - aligned_start,
                );
            }
        } else {
            for dirty_mat in self.dirty[frame].drain(..) {
                let material = materials.get(dirty_mat.id).expect("invalid material");
                let idx = material
                    .material_slot
                    .expect("attempt to update UBO on material without UBO");
                let offset = idx as usize * self.data_size as usize;
                let dst = map.as_ptr().add(offset);

                std::ptr::copy_nonoverlapping(
                    material.material_data.as_ptr(),
                    dst,
                    self.data_size as usize,
                );

                let aligned_start = align_down(offset as u64, atom_mask);
                let aligned_end = align_up(offset as u64 + self.data_size, atom_mask);
                buffer.flush(
                    device,
                    align_down(aligned_start, atom_mask),
                    aligned_end - aligned_start,
                );
            }
        }
    }
}

#[inline]
fn align_down(value: u64, mask: u64) -> u64 {
    value & !mask
}

#[inline]
fn align_up(value: u64, mask: u64) -> u64 {
    (value + mask) & !mask
}
