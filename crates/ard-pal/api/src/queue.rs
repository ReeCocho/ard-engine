use std::time::Duration;

use crate::{
    command_buffer::CommandBuffer,
    context::Context,
    surface::{Surface, SurfaceImage, SurfacePresentError, SurfacePresentSuccess},
    types::{JobStatus, QueueType},
    Backend,
};

/// A queue is used to [`submit`](Queue::submit) commands to the GPU and
/// [`present`](Queue::present) [`surface images`](SurfaceImage).
///
/// # Synchronization
///
/// Because queues are allowed to execute their commands in parallel, they must be synchronized.
/// Each queue follows two rules for synchronization:
///
/// 1. Any command submitted to a queue *must* complete before any other command submitted to the
/// same queue begins.
/// 2. Any queue that accesses a resource *must* wait for all other queues that have commands being
/// executed which access the same resources to finish their execution.
pub struct Queue<B: Backend> {
    ctx: Context<B>,
    ty: QueueType,
}

/// A job represents an in-flight set of commands. It *can* be polled from the CPU for the status
/// of the commands.
pub struct Job<B: Backend> {
    ctx: Context<B>,
    id: B::Job,
}

pub enum SurfacePresentFailure {
    BadImage,
    NoRender,
    Other(String),
}

impl<B: Backend> Queue<B> {
    pub(crate) fn new(ctx: Context<B>, ty: QueueType) -> Self {
        Self { ctx, ty }
    }

    /// Returns the type of queue `self` is.
    #[inline(always)]
    pub fn ty(&self) -> QueueType {
        self.ty
    }

    /// Gets a command buffer to record commands to.
    #[inline(always)]
    pub fn command_buffer<'a>(&self) -> CommandBuffer<'a, B> {
        CommandBuffer {
            queue_ty: self.ty,
            commands: Vec::default(),
        }
    }

    /// Records the commands to a command buffer, and then submits them to the queue.
    ///
    /// # Arguments
    /// - `debug_name` - The backend *should* use the provided debug name for easy identification.
    /// - `commands` - The command buffers to submit.
    #[inline(always)]
    pub fn submit(&self, debug_name: Option<&str>, commands: CommandBuffer<B>) -> Job<B> {
        let id = unsafe {
            self.ctx
                .0
                .submit_commands(self.ty, debug_name, commands.commands, false)
        };

        Job {
            id,
            ctx: self.ctx.clone(),
        }
    }

    /// Submits a command to the command buffer with an additional async compute command buffer.
    ///
    /// The first job returned is for the primary command buffer and the second is for compute.
    ///
    /// # Arguments
    /// - `debug_name` - The backend *should* use the provided debug name for easy identification.
    /// - `commands` - The primary command buffer to submit.
    /// - `compute_commands` - The command buffer to submit to the async compute queue.
    ///
    /// # Panics
    /// - If the primary command buffer is also being submitted to the compute queue.
    #[inline(always)]
    pub fn submit_with_async_compute(
        &self,
        debug_name: Option<&str>,
        commands: CommandBuffer<B>,
        compute_commands: CommandBuffer<B>,
    ) -> (Job<B>, Job<B>) {
        assert_ne!(self.ty, QueueType::Compute);

        let (prim_id, comp_id) = unsafe {
            self.ctx.0.submit_commands_async_compute(
                self.ty,
                debug_name,
                commands.commands,
                compute_commands.commands,
            )
        };

        (
            Job {
                id: prim_id,
                ctx: self.ctx.clone(),
            },
            Job {
                id: comp_id,
                ctx: self.ctx.clone(),
            },
        )
    }

    /// Submits a command buffer for async execution.
    ///
    /// # Arguments
    /// - `debug_name` - The backend *should* use the provided debug name for easy identification.
    /// - `commands` - The command buffers to submit.
    #[inline(always)]
    pub fn submit_async(&self, debug_name: Option<&str>, commands: CommandBuffer<B>) -> Job<B> {
        let id = unsafe {
            self.ctx
                .0
                .submit_commands(self.ty, debug_name, commands.commands, true)
        };

        Job {
            id,
            ctx: self.ctx.clone(),
        }
    }

    /// Presents a rendered [`SurfaceImage`] to a [`Surface`].
    #[inline(always)]
    pub fn present(
        &self,
        surface: &Surface<B>,
        mut image: SurfaceImage<B>,
    ) -> Result<SurfacePresentSuccess, SurfacePresentError<B>> {
        unsafe {
            match self.ctx.0.present_image(&surface.id, &mut image.id) {
                Ok(success) => Ok(success),
                Err(err) => match err {
                    SurfacePresentFailure::BadImage => Err(SurfacePresentError::BadImage(image)),
                    SurfacePresentFailure::NoRender => Err(SurfacePresentError::NoRender(image)),
                    SurfacePresentFailure::Other(msg) => Err(SurfacePresentError::Other(msg)),
                },
            }
        }
    }
}

impl<B: Backend> Job<B> {
    /// Wait's for the job to complete with the given timeout. If `None` is provided, then this
    /// call *must* block as long as possible for the job is finished. Returns the status of the
    /// job by the time the timeout is reached.
    ///
    /// # Arguments
    /// - `timeout` - The time to wait, or `None` if there should be no timeout.
    #[inline(always)]
    pub fn wait_on(&self, timeout: Option<Duration>) -> JobStatus {
        unsafe { self.ctx.0.wait_on(&self.id, timeout) }
    }

    /// Polls the current status of the job without blocking.
    #[inline(always)]
    pub fn poll_status(&self) -> JobStatus {
        unsafe { self.ctx.0.poll_status(&self.id) }
    }
}
