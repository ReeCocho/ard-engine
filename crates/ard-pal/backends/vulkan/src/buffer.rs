use std::{ffi::CString, mem::ManuallyDrop, ptr::NonNull, sync::Arc};

use api::{
    buffer::{BufferCreateError, BufferCreateInfo, BufferViewError},
    types::{BufferUsage, MemoryUsage},
    Backend,
};
use ash::vk::{self, Handle};
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};

use crate::{
    job::Job,
    util::{garbage_collector::Garbage, usage::BufferRegion},
    QueueFamilyIndices, VulkanBackend,
};

pub struct Buffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) _buffer_usage: BufferUsage,
    pub(crate) _memory_usage: MemoryUsage,
    pub(crate) array_elements: usize,
    /// This was the user requested size of each array element.
    pub(crate) size: u64,
    /// This is the per element size after alignment.
    pub(crate) aligned_size: u64,
    pub(crate) ref_counter: BufferRefCounter,
    on_drop: Sender<Garbage>,
}

#[derive(Clone)]
pub(crate) struct BufferRefCounter(Arc<()>);

impl Buffer {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        qfi: &QueueFamilyIndices,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        on_drop: Sender<Garbage>,
        allocator: &mut Allocator,
        limits: &vk::PhysicalDeviceLimits,
        create_info: BufferCreateInfo,
    ) -> Result<Self, BufferCreateError> {
        // Determine memory alignment requirements
        let mut alignment_req = 0;
        if create_info.memory_usage == MemoryUsage::CpuToGpu {
            alignment_req = alignment_req.max(limits.non_coherent_atom_size);
        }
        if create_info
            .buffer_usage
            .contains(BufferUsage::UNIFORM_BUFFER)
        {
            alignment_req = alignment_req.max(limits.min_uniform_buffer_offset_alignment);
        }
        if create_info
            .buffer_usage
            .contains(BufferUsage::STORAGE_BUFFER)
        {
            alignment_req = alignment_req.max(limits.min_storage_buffer_offset_alignment);
        }

        // Round size to a multiple of the alignment
        let aligned_size = match alignment_req {
            0 => create_info.size,
            align => {
                let align_mask = align - 1;
                (create_info.size + align_mask) & !align_mask
            }
        };

        // Create the buffer
        let qfi = qfi.queue_types_to_indices(create_info.queue_types);
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(aligned_size * create_info.array_elements as u64)
            .usage(crate::util::to_vk_buffer_usage(create_info.buffer_usage))
            .sharing_mode(if qfi.len() == 1 {
                vk::SharingMode::EXCLUSIVE
            } else {
                crate::util::to_vk_sharing_mode(create_info.sharing_mode)
            })
            .queue_family_indices(&qfi)
            .build();
        let buffer = match device.create_buffer(&buffer_create_info, None) {
            Ok(buffer) => buffer,
            Err(err) => return Err(BufferCreateError::Other(err.to_string())),
        };

        // Allocate memory
        let mem_reqs = device.get_buffer_memory_requirements(buffer);
        let request = AllocationCreateDesc {
            name: match &create_info.debug_name {
                Some(name) => name,
                None => "unnamed_buffer",
            },
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            requirements: mem_reqs,
            location: crate::util::to_gpu_allocator_memory_location(create_info.memory_usage),
            linear: true,
        };
        let block = match allocator.allocate(&request) {
            Ok(block) => block,
            Err(err) => {
                device.destroy_buffer(buffer, None);
                return Err(BufferCreateError::Other(err.to_string()));
            }
        };

        // Bind buffer to memory
        if let Err(err) = device.bind_buffer_memory(buffer, block.memory(), block.offset()) {
            allocator.free(block).unwrap();
            device.destroy_buffer(buffer, None);
            return Err(BufferCreateError::Other(err.to_string()));
        }

        // Setup debug name is requested
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = debug {
                let name = CString::new(name).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::BUFFER)
                    .object_handle(buffer.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .set_debug_utils_object_name(device.handle(), &name_info)
                    .unwrap();
            }
        }

        Ok(Buffer {
            buffer,
            block: ManuallyDrop::new(block),
            size: create_info.size,
            aligned_size,
            array_elements: create_info.array_elements,
            _buffer_usage: create_info.buffer_usage,
            _memory_usage: create_info.memory_usage,
            on_drop,
            ref_counter: BufferRefCounter::default(),
        })
    }

    #[inline(always)]
    pub(crate) fn offset(&self, array_element: usize) -> u64 {
        self.aligned_size * array_element as u64
    }

    pub(crate) unsafe fn map(
        &self,
        ctx: &VulkanBackend,
        idx: usize,
    ) -> Result<(NonNull<u8>, u64), BufferViewError> {
        // Wait until the last queue that the buffer was used in has finished it's work
        let mut resc_state = ctx.resource_state.write().unwrap();

        // NOTE: The reason we set the usage to `None` is because we have to wait for the previous
        // usage to complete. This implies that no one is using this buffer anymore and thus no
        // waits are further needed.
        if let Some(old) = resc_state.register_buffer(
            &BufferRegion {
                buffer: self.buffer,
                array_elem: idx as u32,
            },
            None,
        ) {
            ctx.wait_on(
                &Job {
                    ty: old.queue,
                    target_value: old.timeline_value,
                },
                None,
            );
        }

        let map = self.block.mapped_ptr().unwrap();
        let map =
            NonNull::new_unchecked((map.as_ptr() as *mut u8).add(self.aligned_size as usize * idx));
        Ok((map, self.size))
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::Buffer {
            buffer: self.buffer,
            array_elements: self.array_elements,
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
        });
    }
}

impl BufferRefCounter {
    #[inline]
    pub fn is_last(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

impl Default for BufferRefCounter {
    #[inline]
    fn default() -> Self {
        BufferRefCounter(Arc::new(()))
    }
}
