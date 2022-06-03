use ash::{vk, vk::DeviceMemory};
use bytemuck::Pod;
use gpu_allocator::vulkan::*;
use gpu_allocator::MemoryLocation;
use std::{mem::ManuallyDrop, ptr::NonNull};

use crate::{context::GraphicsContext, shader_constants::FRAMES_IN_FLIGHT};

pub struct BufferCreateInfo {
    pub ctx: GraphicsContext,
    pub size: u64,
    pub memory_usage: MemoryLocation,
    pub buffer_usage: vk::BufferUsageFlags,
}

pub struct ImageCreateInfo {
    pub ctx: GraphicsContext,
    pub ty: vk::ImageType,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub memory_usage: MemoryLocation,
    pub image_usage: vk::ImageUsageFlags,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub format: vk::Format,
}

pub struct Buffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) size: u64,
    pub(crate) usage: vk::BufferUsageFlags,
    ctx: GraphicsContext,
}

/// Represents a writeable array of some type of object.
pub struct BufferArray<T: Pod> {
    buffer: Buffer,
    map: NonNull<u8>,
    cap: usize,
    _phantom: std::marker::PhantomData<T>,
}

/// Expandable GPU only storage buffer.
pub struct StorageBuffer {
    buffer: Buffer,
    cap: usize,
}

/// Storage buffer designed for fast writes by the CPU with GPU access.
pub struct WriteStorageBuffer {
    buffer: Buffer,
    map: NonNull<u8>,
    cap: usize,
}

/// Represents a uniform buffer object containing some data.
pub struct UniformBuffer {
    buffer: Buffer,
    map: NonNull<u8>,
    aligned_size: u64,
    data_size: u64,
}

pub struct Image {
    pub(crate) image: vk::Image,
    width: u32,
    height: u32,
    mip_levels: u32,
    size: u64,
    format: vk::Format,
    block: ManuallyDrop<Allocation>,
    pub(crate) ctx: GraphicsContext,
}

impl Buffer {
    pub unsafe fn new_staging_buffer(ctx: &GraphicsContext, data: &[u8]) -> Self {
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size: data.len() as u64,
            memory_usage: MemoryLocation::CpuToGpu,
            buffer_usage: vk::BufferUsageFlags::TRANSFER_SRC,
        };

        let mut buffer = Buffer::new(&create_info);
        let map = buffer
            .block
            .mapped_slice_mut()
            .expect("unable to map block memory");

        map[0..data.len()].copy_from_slice(data);

        buffer
    }

    pub unsafe fn new(create_info: &BufferCreateInfo) -> Self {
        // Create buffer
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .flags(vk::BufferCreateFlags::empty())
            .size(create_info.size)
            .usage(create_info.buffer_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let buffer = create_info
            .ctx
            .0
            .device
            .create_buffer(&buffer_create_info, None)
            .expect("unable to create buffer");

        // Get alignment requirement for the buffer
        let mem_reqs = create_info
            .ctx
            .0
            .device
            .get_buffer_memory_requirements(buffer);

        // Allocate memory
        let request = AllocationCreateDesc {
            name: "buffer",
            requirements: mem_reqs,
            location: create_info.memory_usage,
            linear: true,
        };

        let block = create_info
            .ctx
            .0
            .allocator
            .lock()
            .expect("mutex poisoned")
            .allocate(&request)
            .expect("unable to allocate buffer memory");

        // Bind buffer to memory
        create_info
            .ctx
            .0
            .device
            .bind_buffer_memory(buffer, block.memory(), block.offset())
            .expect("unable to bind buffer to memory");

        Buffer {
            block: ManuallyDrop::new(block),
            buffer,
            size: create_info.size,
            usage: create_info.buffer_usage,
            ctx: create_info.ctx.clone(),
        }
    }

    #[inline]
    pub unsafe fn map(&mut self, device: &ash::Device) -> NonNull<u8> {
        NonNull::new_unchecked(
            self.block
                .mapped_ptr()
                .expect("unable to map buffer")
                .as_ptr() as *mut u8,
        )
    }

    #[inline]
    pub unsafe fn flush(&mut self, device: &ash::Device, offset: u64, len: u64) {
        device
            .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                .memory(self.block.memory())
                .offset(offset)
                .size(len)
                .build()])
            .expect("unable to flush buffer memory range");
    }

    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    #[inline]
    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.usage
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.device.destroy_buffer(self.buffer, None);
            self.ctx
                .0
                .allocator
                .lock()
                .expect("mutex poisoned")
                .free(std::mem::ManuallyDrop::take(&mut self.block))
                .expect("unable to free buffer allocation");
        }
    }
}

impl Image {
    pub unsafe fn new(create_info: &ImageCreateInfo) -> Self {
        // Create image
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(create_info.ty)
            .extent(vk::Extent3D {
                width: create_info.width,
                height: create_info.height,
                depth: create_info.depth,
            })
            .mip_levels(create_info.mip_levels)
            .array_layers(create_info.array_layers)
            .format(create_info.format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(create_info.image_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .build();

        let image = create_info
            .ctx
            .0
            .device
            .create_image(&image_create_info, None)
            .expect("unable to create image");

        // Determine size in bytes of the image
        let mem_reqs = create_info
            .ctx
            .0
            .device
            .get_image_memory_requirements(image);

        // Allocate memory
        let request = AllocationCreateDesc {
            name: "buffer",
            requirements: mem_reqs,
            location: create_info.memory_usage,
            linear: true,
        };

        let block = create_info
            .ctx
            .0
            .allocator
            .lock()
            .expect("mutex poisoned")
            .allocate(&request)
            .expect("unable to allocate buffer memory");

        // Bind image to memory
        create_info
            .ctx
            .0
            .device
            .bind_image_memory(image, block.memory(), block.offset())
            .expect("unable to bind image to memory");

        Image {
            block: ManuallyDrop::new(block),
            image,
            mip_levels: create_info.mip_levels,
            width: create_info.width,
            height: create_info.height,
            size: mem_reqs.size,
            format: create_info.format,
            ctx: create_info.ctx.clone(),
        }
    }

    #[inline]
    pub fn image(&self) -> vk::Image {
        self.image
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[inline]
    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn format(&self) -> vk::Format {
        self.format
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.device.destroy_image(self.image, None);
            self.ctx
                .0
                .allocator
                .lock()
                .expect("mutex poisoned")
                .free(std::mem::ManuallyDrop::take(&mut self.block))
                .expect("unable to free image memory allocation");
        }
    }
}

impl<T: Pod> BufferArray<T> {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        initial_cap: usize,
        buffer_usage: vk::BufferUsageFlags,
    ) -> Self {
        let size = (std::mem::size_of::<T>() * initial_cap) as u64;
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size,
            memory_usage: MemoryLocation::CpuToGpu,
            buffer_usage,
        };

        let mut buffer = Buffer::new(&create_info);
        let map = buffer.map(&ctx.0.device);

        BufferArray {
            buffer,
            map,
            cap: initial_cap,
            _phantom: Default::default(),
        }
    }

    #[inline]
    pub fn map(&self) -> NonNull<T> {
        NonNull::<T>::new(self.map.as_ptr() as *mut T).unwrap()
    }

    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.buffer.size
    }

    /// Write a singe object to the buffer.
    #[inline]
    pub unsafe fn write(&mut self, offset: usize, data: T) {
        *(self.map.as_ptr() as *mut T).add(offset) = data;
    }

    /// Write a slice to the buffer.
    #[inline]
    pub unsafe fn write_slice(&mut self, offset: usize, data: &[T]) {
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            (self.map.as_ptr() as *mut T).add(offset),
            data.len(),
        );
    }

    pub unsafe fn invalidate(&mut self, offset: usize, len: usize) {
        let offset = (offset * std::mem::size_of::<T>()) as u64;
        let len = (len * std::mem::size_of::<T>()) as u64;

        match self.buffer.ctx.0.properties.limits.non_coherent_atom_size {
            0 => {
                self.buffer
                    .ctx
                    .0
                    .device
                    .invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(offset)
                        .size(len)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
            size => {
                let atom_mask = size - 1;
                let aligned_offset = offset & !atom_mask;
                let end = (offset + len + atom_mask) & !atom_mask;

                self.buffer
                    .ctx
                    .0
                    .device
                    .invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(aligned_offset)
                        .size(end - aligned_offset)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
        };
    }

    /// Flush a range of objects within the buffer.
    pub unsafe fn flush(&mut self, offset: usize, len: usize) {
        let offset = (offset * std::mem::size_of::<T>()) as u64;
        let len = (len * std::mem::size_of::<T>()) as u64;

        match self.buffer.ctx.0.properties.limits.non_coherent_atom_size {
            0 => {
                self.buffer
                    .ctx
                    .0
                    .device
                    .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(offset)
                        .size(len)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
            size => {
                let atom_mask = size - 1;
                let aligned_offset = offset & !atom_mask;
                let end = (offset + len + atom_mask) & !atom_mask;

                self.buffer
                    .ctx
                    .0
                    .device
                    .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(aligned_offset)
                        .size(end - aligned_offset)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
        };
    }

    /// Expands the capacity of the buffer to meet the new capacity of the buffer. Returns `None`
    /// if the buffer was not expanded, or `Some` containing the new capacity.
    ///
    /// ## Note
    /// The returned new capacity might be larger than `new_cap`.
    pub unsafe fn expand(&mut self, new_cap: usize) -> Option<usize> {
        let mut cap = self.cap;
        while cap < new_cap {
            cap *= 2;
        }

        if cap > self.cap {
            self.cap = cap as usize;
            let create_info = BufferCreateInfo {
                ctx: self.buffer.ctx.clone(),
                size: (std::mem::size_of::<T>() * cap) as u64,
                memory_usage: MemoryLocation::CpuToGpu,
                buffer_usage: self.buffer.usage(),
            };

            self.buffer = Buffer::new(&create_info);
            self.map = NonNull::new_unchecked(
                self.buffer
                    .block
                    .mapped_ptr()
                    .expect("unable to map buffer")
                    .as_ptr() as *mut u8,
            );

            Some(self.cap)
        } else {
            None
        }
    }
}

impl<T: Pod> Drop for BufferArray<T> {
    fn drop(&mut self) {}
}

impl WriteStorageBuffer {
    pub unsafe fn new(ctx: &GraphicsContext, initial_cap: usize) -> Self {
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size: initial_cap as u64,
            memory_usage: MemoryLocation::CpuToGpu,
            buffer_usage: vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::INDIRECT_BUFFER,
        };

        let mut buffer = Buffer::new(&create_info);
        let map = NonNull::new_unchecked(
            buffer
                .block
                .mapped_ptr()
                .expect("unable to map buffer")
                .as_ptr() as *mut u8,
        );

        WriteStorageBuffer {
            buffer,
            map,
            cap: initial_cap,
        }
    }

    #[inline]
    pub fn map(&self) -> NonNull<u8> {
        self.map
    }

    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.buffer.size
    }

    /// Write a singe object to the buffer.
    #[inline]
    pub unsafe fn write<T: Pod>(&self, offset: usize, data: T) {
        assert!(offset + std::mem::size_of::<T>() <= self.buffer.size() as usize);
        *(self.map.as_ptr() as *mut T).add(offset) = data;
    }

    /// Write a slice to the buffer.
    #[inline]
    pub unsafe fn write_slice<T: Pod>(&self, offset: usize, data: &[T]) {
        assert!(offset + (data.len() * std::mem::size_of::<T>()) <= self.buffer.size() as usize);
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            (self.map.as_ptr() as *mut T).add(offset),
            data.len(),
        );
    }

    /// Flush a range of bytes within the buffer.
    pub unsafe fn flush(&self, offset: usize, len: usize) {
        let offset = offset as u64;
        let len = len as u64;

        match self.buffer.ctx.0.properties.limits.non_coherent_atom_size {
            0 => {
                self.buffer
                    .ctx
                    .0
                    .device
                    .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(offset)
                        .size(len)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
            size => {
                let atom_mask = size - 1;
                let aligned_offset = offset & !atom_mask;
                let end = (offset + len + atom_mask) & !atom_mask;

                self.buffer
                    .ctx
                    .0
                    .device
                    .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                        .memory(self.buffer.block.memory())
                        .offset(aligned_offset)
                        .size(end - aligned_offset)
                        .build()])
                    .expect("unable to flush buffer memory range");
            }
        };
    }

    /// Expands the capacity of the buffer to meet the new capacity of the buffer. Returns `None`
    /// if the buffer was not expanded, or `Some` containing the new capacity.
    ///
    /// ## Note
    /// The returned new capacity might be larger than `new_cap`.
    pub unsafe fn expand(&mut self, new_cap: usize) -> Option<usize> {
        let mut cap = self.cap;
        while cap < new_cap {
            cap *= 2;
        }

        if cap > self.cap {
            self.cap = cap as usize;
            let create_info = BufferCreateInfo {
                ctx: self.buffer.ctx.clone(),
                size: cap as u64,
                memory_usage: MemoryLocation::CpuToGpu,
                buffer_usage: self.buffer.usage(),
            };

            self.buffer = Buffer::new(&create_info);
            self.map = NonNull::new_unchecked(
                self.buffer
                    .block
                    .mapped_ptr()
                    .expect("unable to map buffer")
                    .as_ptr() as *mut u8,
            );

            Some(self.cap)
        } else {
            None
        }
    }
}

impl Drop for WriteStorageBuffer {
    fn drop(&mut self) {}
}

impl StorageBuffer {
    pub unsafe fn new(ctx: &GraphicsContext, initial_cap: usize) -> Self {
        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size: initial_cap as u64,
            memory_usage: MemoryLocation::GpuOnly,
            buffer_usage: vk::BufferUsageFlags::STORAGE_BUFFER,
        };

        StorageBuffer {
            buffer: Buffer::new(&create_info),
            cap: initial_cap,
        }
    }

    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.buffer.size
    }

    /// Expands the capacity of the buffer to meet the new capacity of the buffer. Returns `None`
    /// if the buffer was not expanded, or `Some` containing the new capacity.
    ///
    /// ## Note
    /// The returned new capacity might be larger than `new_cap`.
    pub unsafe fn expand(&mut self, new_cap: usize) -> Option<usize> {
        let mut cap = self.cap;
        while cap < new_cap {
            cap *= 2;
        }

        if cap > self.cap {
            self.cap = cap as usize;
            let create_info = BufferCreateInfo {
                ctx: self.buffer.ctx.clone(),
                size: cap as u64,
                memory_usage: MemoryLocation::GpuOnly,
                buffer_usage: self.buffer.usage(),
            };

            self.buffer = Buffer::new(&create_info);

            Some(self.cap)
        } else {
            None
        }
    }
}

impl UniformBuffer {
    pub unsafe fn new<T: Pod>(ctx: &GraphicsContext, data: T) -> Self {
        let min_alignment = ctx.0.properties.limits.min_uniform_buffer_offset_alignment;
        let aligned_size = match min_alignment {
            0 => std::mem::size_of::<T>() as u64,
            align => {
                let align_mask = align - 1;
                (std::mem::size_of::<T>() as u64 + align_mask) & !align_mask
            }
        };

        let create_info = BufferCreateInfo {
            ctx: ctx.clone(),
            size: aligned_size * FRAMES_IN_FLIGHT as u64,
            memory_usage: MemoryLocation::CpuToGpu,
            buffer_usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
        };

        let mut buffer = Buffer::new(&create_info);
        let map = NonNull::new_unchecked(
            buffer
                .block
                .mapped_ptr()
                .expect("unable to map buffer")
                .as_ptr() as *mut u8,
        );

        let mut buffer = UniformBuffer {
            buffer,
            map,
            aligned_size,
            data_size: std::mem::size_of::<T>() as u64,
        };

        for frame in 0..FRAMES_IN_FLIGHT {
            buffer.write(data, frame);
        }

        buffer
    }

    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.data_size
    }

    #[inline]
    pub fn aligned_size(&self) -> u64 {
        self.aligned_size
    }

    #[inline]
    pub unsafe fn write<T: Pod>(&mut self, data: T, frame: usize) {
        debug_assert!(std::mem::size_of::<T>() as u64 <= self.aligned_size);

        *(self.map.as_ptr().add(self.aligned_size as usize * frame) as *mut T) = data;

        self.buffer
            .ctx
            .0
            .device
            .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                .memory(self.buffer.block.memory())
                .offset(self.aligned_size * frame as u64)
                .size(self.aligned_size)
                .build()])
            .expect("unable to flush buffer memory range");
    }
}

impl Drop for UniformBuffer {
    fn drop(&mut self) {}
}

unsafe impl<T: Pod> Send for BufferArray<T> {}

unsafe impl<T: Pod> Sync for BufferArray<T> {}

unsafe impl Send for UniformBuffer {}

unsafe impl Sync for UniformBuffer {}

unsafe impl Send for WriteStorageBuffer {}

unsafe impl Sync for WriteStorageBuffer {}
