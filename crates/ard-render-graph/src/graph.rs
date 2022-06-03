use std::fmt::Debug;

use thiserror::Error;

use crate::{
    buffer::{BufferDescriptor, BufferId},
    context::Context,
    image::{ImageDescriptor, ImageId, SizeGroup, SizeGroupId},
    pass::{Pass, PassDescriptor, PassId},
    AccessType,
};

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum GraphBuildError {
    #[error("size group `{0}` has an invalid configuration")]
    InvalidSizeGroup(SizeGroupId),
    #[error("buffer '{0}' has a size of 0 bytes")]
    InvalidBufferSize(BufferId),
    #[error("graphics pass '{0}' contains no attachments")]
    NoAttachments(PassId),
    #[error("graphics pass '{0}' contains color attachments and/or a depth stencil attachment with mismatching size groups")]
    MixedSizeGroups(PassId),
}

pub struct RenderGraph<C: Context> {
    passes: Vec<PassInfo<C>>,
    resources: RenderGraphResources<C>,
}

pub struct RenderGraphResources<C: Context> {
    size_groups: Vec<SizeGroup>,
    images: Vec<(Option<C::Image>, ImageDescriptor<C>)>,
    buffers: Vec<(Option<C::Buffer>, BufferDescriptor)>,
}

pub struct RenderGraphBuilder<C: Context> {
    size_groups: Vec<SizeGroup>,
    images: Vec<ImageDescriptor<C>>,
    buffers: Vec<BufferDescriptor>,
    passes: Vec<PassDescriptor<C>>,
}

pub struct RenderGraphBuildState {
    /// Describes the state of an image for a pass. First index is the image ID. Second index is
    /// the pointer from 'image_state_ptrs'.
    image_states: Vec<Vec<ImageState>>,
    /// Pointer increases very time an image is encountered. Points to current image state.
    image_state_ptrs: Vec<usize>,
    /// Describes the state of an buffer for a pass. First index is the buffer ID. Second index is
    /// the pointer from 'buffer_state_ptrs'.
    buffer_states: Vec<Vec<BufferState>>,
    /// Pointer increases very time a buffer is encountered. Points to current buffer state.
    buffer_state_ptrs: Vec<usize>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ImageUsage {
    /// Image is either unused or has not been used yet.
    Unused,
    /// Image was used as a color attachment in a graphics pass.
    ColorAttachment,
    /// Image was used as a depth stencil attachment in a graphics pass.
    DepthStencilAttachment,
    /// Image was sampled in a graphics pass.
    Sampled,
    /// Image was used during a compute pass.
    Compute,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BufferUsage {
    /// Buffer is either unused or has not been used yet.
    Unused,
    /// Buffer was used during a graphics pass.
    Graphics,
    /// Buffer was used during a compute pass.
    Compute,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct ImageState {
    pub last: (ImageUsage, AccessType),
    pub current: (ImageUsage, AccessType),
    pub next: (ImageUsage, AccessType),
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct BufferState {
    pub last: (BufferUsage, AccessType),
    pub current: (BufferUsage, AccessType),
    pub next: (BufferUsage, AccessType),
}

struct PassInfo<C: Context> {
    pass: C::Pass,
    toggleable: bool,
}

impl<C: Context> Default for RenderGraphBuilder<C> {
    #[inline]
    fn default() -> Self {
        Self {
            size_groups: Vec::default(),
            images: Vec::default(),
            buffers: Vec::default(),
            passes: Vec::default(),
        }
    }
}

impl<C: Context> RenderGraph<C> {
    pub fn run(&mut self, ctx: &mut C, state: &mut C::State, command_buffer: &C::CommandBuffer) {
        for pass in &mut self.passes {
            pass.pass
                .run(command_buffer, ctx, state, &mut self.resources);
        }
    }

    #[inline]
    pub fn get_pass(&self, id: PassId) -> &C::Pass {
        &self.passes[id.0 as usize].pass
    }

    #[inline]
    pub fn get_pass_mut(&mut self, id: PassId) -> &mut C::Pass {
        &mut self.passes[id.0 as usize].pass
    }

    #[inline]
    pub fn resources(&self) -> &RenderGraphResources<C> {
        &self.resources
    }

    #[inline]
    pub fn resources_mut(&mut self) -> &mut RenderGraphResources<C> {
        &mut self.resources
    }

    pub fn update_size_group(&mut self, ctx: &mut C, id: SizeGroupId, new_size: SizeGroup) {
        self.resources.size_groups[id.0 as usize] = new_size;

        // Determine which images need updates
        let mut to_update = Vec::default();
        for (i, (image, descriptor)) in self.resources.images.iter().enumerate() {
            if image.is_some() && descriptor.size_group == id {
                to_update.push(i);
            }
        }

        // Update the images
        for idx in to_update {
            self.resources.images[idx].0 =
                Some(ctx.create_image(&self.resources.images[idx].1, &self.resources));
        }
    }
}

impl<C: Context> RenderGraphResources<C> {
    #[inline]
    pub fn get_size_group(&self, id: SizeGroupId) -> &SizeGroup {
        &self.size_groups[id.0 as usize]
    }

    #[inline]
    pub fn get_image(&self, id: ImageId) -> Option<&C::Image> {
        self.images[id.0 as usize].0.as_ref()
    }

    #[inline]
    pub fn get_buffer(&self, id: BufferId) -> Option<&C::Buffer> {
        self.buffers[id.0 as usize].0.as_ref()
    }

    #[inline]
    pub fn get_image_mut(&mut self, id: ImageId) -> Option<&mut C::Image> {
        self.images[id.0 as usize].0.as_mut()
    }

    #[inline]
    pub fn get_buffer_mut(&mut self, id: BufferId) -> Option<&mut C::Buffer> {
        self.buffers[id.0 as usize].0.as_mut()
    }
}

impl RenderGraphBuildState {
    #[inline]
    pub fn get_image_state(&self, id: ImageId) -> Option<&ImageState> {
        if self.image_states[id.0 as usize].is_empty() {
            None
        } else {
            let idx = self.image_state_ptrs[id.0 as usize];
            Some(&self.image_states[id.0 as usize][idx])
        }
    }

    #[inline]
    pub fn get_buffer_state(&self, id: BufferId) -> Option<&BufferState> {
        if self.buffer_states[id.0 as usize].is_empty() {
            None
        } else {
            let idx = self.buffer_state_ptrs[id.0 as usize];
            Some(&self.buffer_states[id.0 as usize][idx])
        }
    }
}

impl<C: Context> RenderGraphBuilder<C> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn get_size_group(&self, id: SizeGroupId) -> &SizeGroup {
        &self.size_groups[id.0 as usize]
    }

    #[inline]
    pub fn add_size_group(&mut self, size_group: SizeGroup) -> SizeGroupId {
        self.size_groups.push(size_group);
        SizeGroupId(self.size_groups.len() as u32 - 1)
    }

    #[inline]
    pub fn add_buffer(&mut self, descriptor: BufferDescriptor) -> BufferId {
        self.buffers.push(descriptor);
        BufferId(self.buffers.len() as u32 - 1)
    }

    #[inline]
    pub fn add_image(&mut self, descriptor: ImageDescriptor<C>) -> ImageId {
        self.images.push(descriptor);
        ImageId(self.images.len() as u32 - 1)
    }

    #[inline]
    pub fn add_pass(&mut self, descriptor: PassDescriptor<C>) -> PassId {
        self.passes.push(descriptor);
        PassId(self.passes.len() as u32 - 1)
    }

    pub fn build(self, ctx: &mut C) -> Result<RenderGraph<C>, GraphBuildError> {
        // Error checking
        if let Some(err) = self.verify() {
            return Err(err);
        }

        let mut graph_state = RenderGraphBuildState {
            image_states: vec![Vec::default(); self.images.len()],
            image_state_ptrs: vec![0; self.images.len()],
            buffer_states: vec![Vec::default(); self.buffers.len()],
            buffer_state_ptrs: vec![0; self.buffers.len()],
        };

        // Construct the states for buffers and images
        fn update_image_state(
            states: &mut Vec<ImageState>,
            current_usage: ImageUsage,
            access_type: AccessType,
        ) {
            let mut state = ImageState {
                last: (ImageUsage::Unused, AccessType::Read),
                current: (current_usage, access_type),
                next: (ImageUsage::Unused, AccessType::Read),
            };

            // Determine out last usage and update last states next usage
            if !states.is_empty() {
                let last_state = states.last_mut().unwrap();
                last_state.next = (current_usage, access_type);
                state.last = last_state.current;
            }

            states.push(state);
        }

        fn update_buffer_state(
            states: &mut Vec<BufferState>,
            current_usage: BufferUsage,
            access_type: AccessType,
        ) {
            let mut state = BufferState {
                last: (BufferUsage::Unused, AccessType::Read),
                current: (current_usage, access_type),
                next: (BufferUsage::Unused, AccessType::Read),
            };

            // Determine out last usage and update last states next usage
            if !states.is_empty() {
                let last_state = states.last_mut().unwrap();
                last_state.next = (current_usage, access_type);
                state.last = last_state.current;
            }

            states.push(state);
        }

        for pass in &self.passes {
            match pass {
                PassDescriptor::RenderPass {
                    color_attachments,
                    depth_stencil_attachment,
                    buffers,
                    images,
                    ..
                } => {
                    for attachment in color_attachments {
                        let states = &mut graph_state.image_states[attachment.image.0 as usize];
                        update_image_state(
                            states,
                            ImageUsage::ColorAttachment,
                            AccessType::ReadWrite,
                        );
                    }

                    for image in images {
                        let states = &mut graph_state.image_states[image.image.0 as usize];
                        update_image_state(states, ImageUsage::Sampled, image.access);
                    }

                    if let Some(attachment) = depth_stencil_attachment {
                        let states = &mut graph_state.image_states[attachment.image.0 as usize];
                        update_image_state(
                            states,
                            ImageUsage::DepthStencilAttachment,
                            AccessType::ReadWrite,
                        );
                    }

                    for buffer in buffers {
                        let states = &mut graph_state.buffer_states[buffer.buffer.0 as usize];
                        update_buffer_state(states, BufferUsage::Graphics, buffer.access);
                    }
                }
                PassDescriptor::ComputePass {
                    images, buffers, ..
                } => {
                    for image in images {
                        let states = &mut graph_state.image_states[image.image.0 as usize];
                        update_image_state(states, ImageUsage::Compute, image.access);
                    }

                    for buffer in buffers {
                        let states = &mut graph_state.buffer_states[buffer.buffer.0 as usize];
                        update_buffer_state(states, BufferUsage::Compute, buffer.access);
                    }
                }
                PassDescriptor::CPUPass { .. } => {}
            }
        }

        // Creating images and buffers. Don't waste memory on unused ones
        let mut resources = RenderGraphResources::<C> {
            size_groups: self.size_groups,
            images: Vec::with_capacity(self.images.len()),
            buffers: Vec::with_capacity(self.buffers.len()),
        };

        for (idx, descriptor) in self.images.into_iter().enumerate() {
            if !graph_state.image_states[idx].is_empty() {
                resources
                    .images
                    .push((Some(ctx.create_image(&descriptor, &resources)), descriptor));
            } else {
                resources.images.push((None, descriptor));
            }
        }

        for (idx, descriptor) in self.buffers.into_iter().enumerate() {
            if !graph_state.buffer_states[idx].is_empty() {
                resources
                    .buffers
                    .push((Some(ctx.create_buffer(&descriptor, &resources)), descriptor));
            } else {
                resources.buffers.push((None, descriptor));
            }
        }

        // Construct passes
        let mut passes = Vec::with_capacity(self.passes.len());

        for descriptor in self.passes {
            // Update image and buffer pointers
            match &descriptor {
                PassDescriptor::RenderPass {
                    color_attachments,
                    depth_stencil_attachment,
                    buffers,
                    toggleable,
                    ..
                } => {
                    passes.push(PassInfo {
                        pass: ctx.create_pass(&descriptor, &graph_state, &resources),
                        toggleable: *toggleable,
                    });

                    for attachment in color_attachments {
                        graph_state.image_state_ptrs[attachment.image.0 as usize] += 1;
                    }

                    if let Some(attachment) = depth_stencil_attachment {
                        graph_state.image_state_ptrs[attachment.image.0 as usize] += 1;
                    }

                    for buffer in buffers {
                        graph_state.buffer_state_ptrs[buffer.buffer.0 as usize] += 1;
                    }
                }
                PassDescriptor::ComputePass {
                    images,
                    buffers,
                    toggleable,
                    ..
                } => {
                    passes.push(PassInfo {
                        pass: ctx.create_pass(&descriptor, &graph_state, &resources),
                        toggleable: *toggleable,
                    });

                    for image in images {
                        graph_state.image_state_ptrs[image.image.0 as usize] += 1;
                    }

                    for buffer in buffers {
                        graph_state.buffer_state_ptrs[buffer.buffer.0 as usize] += 1;
                    }
                }
                PassDescriptor::CPUPass { toggleable, .. } => {
                    passes.push(PassInfo {
                        pass: ctx.create_pass(&descriptor, &graph_state, &resources),
                        toggleable: *toggleable,
                    });
                }
            }
        }

        Ok(RenderGraph::<C> { resources, passes })
    }

    /// Helper to verify the state of the graph is consistent.
    fn verify(&self) -> Option<GraphBuildError> {
        // Check all size groups
        for (id, group) in self.size_groups.iter().enumerate() {
            if group.width == 0
                || group.height == 0
                || group.mip_levels == 0
                || group.array_layers == 0
            {
                return Some(GraphBuildError::InvalidSizeGroup(SizeGroupId(id as u32)));
            }
        }

        // Check all buffers
        for (id, buffer) in self.buffers.iter().enumerate() {
            if buffer.size == 0 {
                return Some(GraphBuildError::InvalidBufferSize(BufferId(id as u32)));
            }
        }

        // Verify size groups are consistent within graphics passes and that graphics passes have
        // at least one attachment
        for (id, pass) in self.passes.iter().enumerate() {
            if let PassDescriptor::RenderPass {
                color_attachments,
                depth_stencil_attachment,
                ..
            } = pass
            {
                let mut size_group = None;

                // Find the size group of a single image so that we can compare it against
                // all the other images. If we don't find a size group, then we know we have
                // no attachments, which is also an error
                if let Some(image) = color_attachments.first() {
                    let image = &self.images[image.image.0 as usize];
                    size_group = Some(image.size_group);
                }

                if let Some(descriptor) = depth_stencil_attachment {
                    let image_id = descriptor.image;
                    let image = &self.images[image_id.0 as usize];
                    size_group = Some(image.size_group);
                }

                // Compare size groups
                if let Some(size_group) = size_group {
                    for image in color_attachments {
                        let image = &self.images[image.image.0 as usize];

                        if image.size_group != size_group {
                            return Some(GraphBuildError::MixedSizeGroups(PassId(id as u32)));
                        }
                    }

                    if let Some(descriptor) = depth_stencil_attachment {
                        let image = &self.images[descriptor.image.0 as usize];

                        if image.size_group != size_group {
                            return Some(GraphBuildError::MixedSizeGroups(PassId(id as u32)));
                        }
                    }
                }
                // No size group means no attachments
                else {
                    return Some(GraphBuildError::NoAttachments(PassId(id as u32)));
                }
            }
        }

        None
    }
}

impl<C: Context> Debug for RenderGraph<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderGraph").finish()
    }
}
