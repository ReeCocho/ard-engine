use std::sync::Arc;

use crate::{queue::Queue, types::QueueType, Backend};

/// The context is the entry point for Pal. It is used to create all other Pal objects.
///
/// The context also provides you with a selection of four [`Queues`](Queue).
pub struct Context<B: Backend>(pub(crate) Arc<B>);

impl<B: Backend> Context<B> {
    /// Creates a new Pal instance.
    ///
    /// # Arguments
    ///
    /// - `backend` - A backend object selected based on your system. See `/backends/` for a
    /// selection to choose from.
    #[inline(always)]
    pub fn new(backend: B) -> Self {
        Self(Arc::new(backend))
    }

    /// Gets a reference to the primary queue.
    ///
    /// # Supported Commands
    ///
    /// - [`render_pass`](crate::command_buffer::CommandBuffer::render_pass)
    /// - [`compute_pass`](crate::command_buffer::CommandBuffer::compute_pass)
    /// - [`copy_buffer_to_buffer`](crate::command_buffer::CommandBuffer::copy_buffer_to_buffer)
    /// - [`copy_buffer_to_texture`](crate::command_buffer::CommandBuffer::copy_buffer_to_texture)
    /// - [`copy_texture_to_buffer`](crate::command_buffer::CommandBuffer::copy_texture_to_buffer)
    #[inline(always)]
    pub fn main(&self) -> Queue<B> {
        Queue::new(self.clone(), QueueType::Main)
    }

    /// Gets a reference to the async transfer queue.
    ///
    /// # Supported Commands
    ///
    /// - [`copy_buffer_to_buffer`](crate::command_buffer::CommandBuffer::copy_buffer_to_buffer)
    /// - [`copy_buffer_to_texture`](crate::command_buffer::CommandBuffer::copy_buffer_to_texture)
    /// - [`copy_texture_to_buffer`](crate::command_buffer::CommandBuffer::copy_texture_to_buffer)
    #[inline(always)]
    pub fn transfer(&self) -> Queue<B> {
        Queue::new(self.clone(), QueueType::Transfer)
    }

    /// Gets a reference to the async compute queue.
    ///
    /// # Supported Commands
    ///
    /// - [`compute_pass`](crate::command_buffer::CommandBuffer::compute_pass)
    #[inline(always)]
    pub fn compute(&self) -> Queue<B> {
        Queue::new(self.clone(), QueueType::Compute)
    }

    /// Gets a reference to the presentation queue. The present queue can only be used to submit
    /// surface images for presentation using the [`present`](Queue::present) function.
    #[inline(always)]
    pub fn present(&self) -> Queue<B> {
        Queue::new(self.clone(), QueueType::Present)
    }
}

impl<B: Backend> Clone for Context<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
