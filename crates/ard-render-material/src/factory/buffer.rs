use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, Context, MemoryUsage, QueueTypes, SharingMode,
};
use ard_render_base::{ecs::Frame, resource::ResourceAllocator, FRAMES_IN_FLIGHT};

use crate::material_instance::{MaterialInstance, MaterialInstanceResource};

/// Contains material data for a size of object.
pub struct MaterialBuffer {
    /// How big a single object can be from this material buffer.
    data_size: u64,
    /// The actual material buffer.
    buffer: Buffer,
    /// How many object instances can currently fit in the buffer.
    cap: usize,
    /// Slot ID counter for allocated objects.
    slot_counter: usize,
    /// List of free slots.
    free: Vec<MaterialSlot>,
    /// For each frame in flight, marks the dirty materials in this buffer.
    dirty: [Vec<MaterialInstance>; FRAMES_IN_FLIGHT],
}

#[derive(Debug, Copy, Clone)]
pub struct MaterialSlot(u16);

impl MaterialBuffer {
    pub fn new(ctx: Context, debug_name: String, data_size: u64, default_capacity: usize) -> Self {
        MaterialBuffer {
            data_size,
            dirty: std::array::from_fn(|_| Vec::default()),
            cap: default_capacity,
            free: Vec::default(),
            slot_counter: 0,
            buffer: Buffer::new(
                ctx,
                BufferCreateInfo {
                    size: data_size * default_capacity as u64,
                    array_elements: FRAMES_IN_FLIGHT,
                    buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::DEVICE_ADDRESS,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some(debug_name),
                },
            )
            .unwrap(),
        }
    }

    #[inline(always)]
    pub fn data_size(&self) -> u64 {
        self.data_size
    }

    #[inline(always)]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn allocate(&mut self) -> MaterialSlot {
        self.free.pop().unwrap_or_else(|| {
            self.slot_counter += 1;
            MaterialSlot(self.slot_counter as u16 - 1)
        })
    }

    pub fn free(&mut self, slot: MaterialSlot) {
        self.free.push(slot);
    }

    /// Marks a particular material instance as having been modified so it's properties can be
    /// written to the material buffer.
    pub fn mark_dirty(&mut self, material: &MaterialInstance) {
        self.dirty
            .iter_mut()
            .for_each(|list| list.push(material.clone()));
    }

    /// Flushes dirty material instances.
    pub fn flush(
        &mut self,
        frame: Frame,
        materials: &ResourceAllocator<MaterialInstanceResource>,
        on_flush: impl Fn(&mut [u8], &MaterialInstanceResource),
    ) {
        if self.slot_counter > self.cap {
            panic!("Material buffer '{}' exceeded capacity.", self.data_size);
        }

        // Flush dirty values
        let mut view = self.buffer.write(frame.into()).unwrap();
        self.dirty[usize::from(frame)]
            .drain(..)
            .for_each(|material_instance| {
                on_flush(
                    &mut view,
                    // Safe to unwrap since the resource must exist if we have a handle to it
                    materials.get(material_instance.id()).unwrap(),
                );
            });
    }
}

impl From<MaterialSlot> for usize {
    fn from(value: MaterialSlot) -> Self {
        value.0 as usize
    }
}

impl From<MaterialSlot> for u64 {
    fn from(value: MaterialSlot) -> Self {
        value.0 as u64
    }
}

impl From<MaterialSlot> for u32 {
    fn from(value: MaterialSlot) -> Self {
        value.0 as u32
    }
}

impl From<MaterialSlot> for u16 {
    fn from(value: MaterialSlot) -> Self {
        value.0
    }
}
