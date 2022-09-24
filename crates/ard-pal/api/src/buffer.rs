use std::{
    cell::{Ref, RefCell, RefMut},
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{context::Context, types::*, Backend};
use thiserror::Error;

pub struct BufferCreateInfo {
    /// The size in bytes of the buffer to create.
    pub size: u64,
    /// How many array elements this buffer supports. Each array element is of the same size as
    /// `size`.
    pub array_elements: usize,
    /// Describes the supported usage types of this buffer.
    pub buffer_usage: BufferUsage,
    /// Describes what memory operations are supported by this buffer.
    pub memory_usage: MemoryUsage,
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum BufferCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum BufferViewError {
    #[error("an error has occured: {0}")]
    Other(String),
}

/// A GPU memory buffer. For the purposes of synchronization, this is considered a resource.
pub struct Buffer<B: Backend> {
    ctx: Context<B>,
    size: u64,
    buffer_usage: BufferUsage,
    memory_usage: MemoryUsage,
    array_elements: usize,
    debug_name: Option<String>,
    view_check: RefCell<()>,
    pub(crate) id: B::Buffer,
}

pub struct BufferReadView<'a, B: Backend> {
    _buffer: &'a Buffer<B>,
    slice: &'a [u8],
    _borrow: Ref<'a, ()>,
}

pub struct BufferWriteView<'a, B: Backend> {
    ctx: Context<B>,
    idx: usize,
    buffer: &'a Buffer<B>,
    slice: &'a mut [u8],
    _borrow: RefMut<'a, ()>,
}

impl<B: Backend> Buffer<B> {
    /// Creates a new buffer.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `create_info` - Describes the buffer to create.
    ///
    /// # Panics
    /// - If `create_info.size` is `0`.
    /// - If `create_info.array_elements` is `0`.
    #[inline(always)]
    pub fn new(ctx: Context<B>, create_info: BufferCreateInfo) -> Result<Self, BufferCreateError> {
        assert_ne!(create_info.size, 0, "buffer size cannot be zero");
        assert_ne!(
            create_info.array_elements, 0,
            "buffer array elements cannot be zero"
        );
        let size = create_info.size;
        let buffer_usage = create_info.buffer_usage;
        let memory_usage = create_info.memory_usage;
        let array_elements = create_info.array_elements;
        let debug_name = create_info.debug_name.clone();
        let id = unsafe { ctx.0.create_buffer(create_info)? };
        Ok(Self {
            ctx,
            id,
            size,
            memory_usage,
            buffer_usage,
            array_elements,
            view_check: RefCell::default(),
            debug_name,
        })
    }

    /// Creates a new staging buffer. A staging buffer is typically used to transfer data from the
    /// CPU to a [`GpuOnly`](MemoryUsage::GpuOnly) buffer.
    ///
    /// Staging buffers are not a special kind of buffer. This is simply a helper function to do
    /// the following:
    /// 1. Create a buffer that is [`TRANSFER_SRC`](BufferUsage) and
    /// [`CpuToGpu`](MemoryUsage::CpuToGpu) with a single array element.
    /// 2. Copy the `data` to the buffer.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `debug_name` - The backend *should* use the provided debug name for easy identification.
    /// - `data` - The data to upload to the buffer.
    ///
    /// # Panics
    /// - If `data.is_empty()`.
    pub fn new_staging(
        ctx: Context<B>,
        debug_name: Option<String>,
        data: &[u8],
    ) -> Result<Buffer<B>, BufferCreateError> {
        let create_info = BufferCreateInfo {
            size: data.len() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::TRANSFER_SRC,
            memory_usage: MemoryUsage::CpuToGpu,
            debug_name,
        };
        let buffer = Buffer::new(ctx, create_info)?;
        let mut view = buffer.write(0).unwrap();
        view.copy_from_slice(&data);
        std::mem::drop(view);
        Ok(buffer)
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::Buffer {
        &self.id
    }

    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[inline(always)]
    pub fn buffer_usage(&self) -> BufferUsage {
        self.buffer_usage
    }

    #[inline(always)]
    pub fn memory_usage(&self) -> MemoryUsage {
        self.memory_usage
    }

    /// Provides a view into the buffer for read only operations.
    ///
    /// # Arguments
    /// - `idx` - The array element of the buffer to view.
    ///
    /// # Synchronization
    /// The backend *must* guarantee that the buffer is not being read or written to by any
    /// in-flight commands by the time the user has access to the buffer map.
    ///
    /// # Panics
    /// - If the buffer is not mappable. That is, the buffer was created with `memory_usage` equal
    /// to [`MemoryUsage::CpuToGpu`] or [`MemoryUsage::GpuToCpu`].
    /// - If `idx` is not a valid array element of the buffer.
    /// - If the buffer is already being viewed mutably.
    #[inline(always)]
    pub fn read(&self, idx: usize) -> Result<BufferReadView<B>, BufferViewError> {
        assert!(idx < self.array_elements, "`idx` is out of bounds");
        assert!(
            self.memory_usage == MemoryUsage::CpuToGpu
                || self.memory_usage == MemoryUsage::GpuToCpu,
            "buffer is not mappable"
        );

        let borrow = self.view_check.borrow();
        let (map, len) = unsafe {
            let res = self.ctx.0.map_memory(&self.id, idx)?;
            self.ctx.0.invalidate_range(&self.id, idx);
            res
        };
        Ok(BufferReadView {
            _buffer: self,
            slice: unsafe { std::slice::from_raw_parts(map.as_ptr(), len as usize) },
            _borrow: borrow,
        })
    }

    /// Provides a view into the buffer for read and write operations.
    ///
    /// See [`read`](Buffer::read) for synchronization requirements and panics.
    ///
    /// # Arguments
    /// - `idx` - The array element of the buffer to view.
    #[inline(always)]
    pub fn write(&self, idx: usize) -> Result<BufferWriteView<B>, BufferViewError> {
        let borrow = self.view_check.borrow_mut();
        let (map, len) = unsafe {
            let res = self.ctx.0.map_memory(&self.id, idx)?;
            self.ctx.0.invalidate_range(&self.id, idx);
            res
        };
        Ok(BufferWriteView {
            idx,
            ctx: self.ctx.clone(),
            buffer: self,
            slice: unsafe { std::slice::from_raw_parts_mut(map.as_ptr(), len as usize) },
            _borrow: borrow,
        })
    }

    /// Expands a buffer to fit the provided size, or does nothing if the buffer is already the
    /// appropriate size. If the buffer is expanded, the new buffer is returned.
    ///
    /// # Arguments
    /// - `buffer` - The buffer to expand.
    /// - `new_size` - The required size.
    /// - `preserve` - If `true`, the original contents of the buffer will be copied into the new
    /// buffer
    ///
    /// # Panics
    /// - If `buffer` does not support read and write operations.
    pub fn expand(buffer: &Self, new_size: u64, preserve: bool) -> Option<Self> {
        // Already the appropriate size
        if buffer.size >= new_size {
            return None;
        }

        // Determine a new size that fits the requirement
        let mut final_size = buffer.size;
        while final_size < new_size {
            final_size *= 2;
        }

        // Create a new buffer with the size
        let new_buffer = Self::new(
            buffer.ctx.clone(),
            BufferCreateInfo {
                size: final_size,
                array_elements: buffer.array_elements,
                buffer_usage: buffer.buffer_usage,
                memory_usage: buffer.memory_usage,
                debug_name: buffer.debug_name.clone(),
            },
        )
        .unwrap();

        // If preserve is requested, copy the data
        if preserve {
            for i in 0..buffer.array_elements {
                let len = buffer.size() as usize;
                let old_view = buffer.read(i).unwrap();
                let mut new_view = new_buffer.write(i).unwrap();
                new_view[..len].copy_from_slice(&old_view);
            }
        }

        Some(new_buffer)
    }
}

impl<B: Backend> Drop for Buffer<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_buffer(&mut self.id);
        }
    }
}

impl<'a, B: Backend> BufferReadView<'a, B> {
    #[inline]
    pub unsafe fn into_raw(self) -> (NonNull<u8>, usize) {
        let ptr = NonNull::new_unchecked(self.slice.as_ptr() as *mut u8);
        let len = self.slice.len();
        (ptr, len)
    }
}

impl<'a, B: Backend> Deref for BufferReadView<'a, B> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.slice
    }
}

impl<'a, B: Backend> BufferWriteView<'a, B> {
    #[inline]
    pub unsafe fn into_raw(self) -> (NonNull<u8>, usize) {
        let ptr = NonNull::new_unchecked(self.slice.as_mut_ptr());
        let len = self.slice.len();
        (ptr, len)
    }
}

impl<'a, B: Backend> Deref for BufferWriteView<'a, B> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.slice
    }
}

impl<'a, B: Backend> DerefMut for BufferWriteView<'a, B> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.slice
    }
}

impl<'a, B: Backend> Drop for BufferWriteView<'a, B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.flush_range(&self.buffer.id, self.idx);
        }
    }
}
