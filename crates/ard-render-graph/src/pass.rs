use std::fmt::Display;

use crate::{
    buffer::BufferAccessDescriptor,
    context::Context,
    graph::RenderGraphResources,
    image::{ImageAccessDecriptor, ImageId},
    Operations,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PassId(pub(crate) u32);

impl PassId {
    /// Creates an invalid pass id.
    #[inline]
    pub const fn invalid() -> Self {
        PassId(u32::MAX)
    }
}

pub trait Pass<C: Context> {
    fn run(
        &mut self,
        command_buffer: &<C as Context>::CommandBuffer,
        ctx: &mut C,
        state: &mut C::State,
        resources: &mut RenderGraphResources<C>,
    );
}

#[derive(Debug, Copy, Clone)]
pub struct ColorAttachmentDescriptor {
    pub image: ImageId,
    pub ops: Operations<[f32; 4]>,
}

#[derive(Debug, Copy, Clone)]
pub struct DepthStencilAttachmentDescriptor {
    pub image: ImageId,
    pub ops: Operations<(f32, u32)>,
}

pub type PassFn<C> = fn(
    &mut C,
    &mut <C as Context>::State,
    &<C as Context>::CommandBuffer,
    &mut <C as Context>::Pass,
    &mut RenderGraphResources<C>,
) -> ();

pub enum PassDescriptor<C: Context> {
    RenderPass {
        toggleable: bool,
        color_attachments: Vec<ColorAttachmentDescriptor>,
        depth_stencil_attachment: Option<DepthStencilAttachmentDescriptor>,
        buffers: Vec<BufferAccessDescriptor>,
        code: PassFn<C>,
    },
    ComputePass {
        toggleable: bool,
        images: Vec<ImageAccessDecriptor>,
        buffers: Vec<BufferAccessDescriptor>,
        code: PassFn<C>,
    },
    CPUPass {
        toggleable: bool,
        code: PassFn<C>,
    },
}

impl Display for PassId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
