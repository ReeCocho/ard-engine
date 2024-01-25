use std::collections::{HashMap, HashSet};

use api::{
    render_pass::{ColorAttachmentSource, RenderPassDescriptor},
    types::LoadOp,
};
use ash::vk;
use bytemuck::{Pod, Zeroable};
use dashmap::DashMap;
use rustc_hash::FxHashMap;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DrawIndexedIndirect {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

unsafe impl Pod for DrawIndexedIndirect {}
unsafe impl Zeroable for DrawIndexedIndirect {}

#[derive(Default)]
pub(crate) struct RenderPassCache {
    passes: DashMap<VkRenderPassDescriptor, VkRenderPass>,
}

#[derive(Default)]
pub(crate) struct FramebufferCache {
    /// Maps a render pass to their framebuffers.
    pass_to_framebuffers: DashMap<vk::RenderPass, Framebuffers>,
    /// Maps images to what render passes have framebuffers containing them.
    image_to_pass: DashMap<vk::ImageView, HashSet<vk::RenderPass>>,
}

#[derive(Default)]
pub(crate) struct Framebuffers {
    pass: vk::RenderPass,
    /// Maps an ordered set of images to a framebuffer.
    framebuffers: FxHashMap<Vec<vk::ImageView>, vk::Framebuffer>,
}

#[derive(Default, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) struct VkRenderPass {
    pub pass: vk::RenderPass,
    pub samples: vk::SampleCountFlags,
}

#[derive(Default, Hash, PartialEq, Eq)]
pub(crate) struct VkRenderPassDescriptor {
    pub color_attachments: Vec<VkAttachment>,
    pub color_resolve_attachments: Vec<VkAttachment>,
    pub depth_stencil_attachment: Option<VkAttachment>,
    pub depth_stencil_resolve_attachment: Option<VkAttachment>,
}

#[derive(Hash, PartialEq, Eq)]
pub(crate) struct VkAttachment {
    pub image_format: vk::Format,
    pub initial_layout: vk::ImageLayout,
    pub final_layout: vk::ImageLayout,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub resolve_src: usize,
}

impl RenderPassCache {
    /// Checks if a compatible render pass is in the cache. If it is, it is returned. Otherwise,
    /// a new render pass is created and returned.
    pub fn get(
        &self,
        device: &ash::Device,
        pass: &RenderPassDescriptor<crate::VulkanBackend>,
    ) -> VkRenderPass {
        let descriptor = VkRenderPassDescriptor::from_descriptor(pass);
        *self.passes.entry(descriptor).or_insert_with(|| {
            let mut sample_count = vk::SampleCountFlags::TYPE_1;

            // Create attachment descriptors
            let mut attachments = Vec::with_capacity(
                pass.color_attachments.len() + pass.color_resolve_attachments.len(),
            );
            for attachment in &pass.color_attachments {
                let layout = match &attachment.source {
                    ColorAttachmentSource::SurfaceImage(_) => vk::ImageLayout::PRESENT_SRC_KHR,
                    ColorAttachmentSource::Texture { .. } => {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    }
                    ColorAttachmentSource::CubeMap { .. } => {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    }
                };

                sample_count = crate::util::to_vk_sample_count(attachment.samples);

                attachments.push(
                    vk::AttachmentDescription2::builder()
                        .initial_layout(match attachment.load_op {
                            LoadOp::Load => layout,
                            LoadOp::DontCare => layout,
                            LoadOp::Clear(_) => layout,
                        })
                        .final_layout(layout)
                        .load_op(crate::util::to_vk_load_op(attachment.load_op))
                        .store_op(crate::util::to_vk_store_op(attachment.store_op))
                        .format(match &attachment.source {
                            ColorAttachmentSource::SurfaceImage(image) => image.internal().format(),
                            ColorAttachmentSource::Texture { texture, .. } => {
                                texture.internal().format
                            }
                            ColorAttachmentSource::CubeMap { cube_map, .. } => {
                                cube_map.internal().format
                            }
                        })
                        .samples(sample_count)
                        .build(),
                );
            }

            for attachment in &pass.color_resolve_attachments {
                let layout = match &attachment.dst {
                    ColorAttachmentSource::SurfaceImage(_) => vk::ImageLayout::PRESENT_SRC_KHR,
                    ColorAttachmentSource::Texture { .. } => {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    }
                    ColorAttachmentSource::CubeMap { .. } => {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    }
                };

                attachments.push(
                    vk::AttachmentDescription2::builder()
                        .initial_layout(match attachment.load_op {
                            LoadOp::Load => layout,
                            LoadOp::DontCare => layout,
                            LoadOp::Clear(_) => layout,
                        })
                        .final_layout(layout)
                        .load_op(crate::util::to_vk_load_op(attachment.load_op))
                        .store_op(crate::util::to_vk_store_op(attachment.store_op))
                        .format(match &attachment.dst {
                            ColorAttachmentSource::SurfaceImage(image) => image.internal().format(),
                            ColorAttachmentSource::Texture { texture, .. } => {
                                texture.internal().format
                            }
                            ColorAttachmentSource::CubeMap { cube_map, .. } => {
                                cube_map.internal().format
                            }
                        })
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .build(),
                );
            }

            if let Some(attachment) = &pass.depth_stencil_attachment {
                sample_count = crate::util::to_vk_sample_count(attachment.samples);

                attachments.push(
                    vk::AttachmentDescription2::builder()
                        .initial_layout(match attachment.load_op {
                            LoadOp::Load => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                            LoadOp::DontCare => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                            LoadOp::Clear(_) => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                        })
                        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .load_op(crate::util::to_vk_load_op(attachment.load_op))
                        .store_op(crate::util::to_vk_store_op(attachment.store_op))
                        .format(attachment.texture.internal().format)
                        .samples(sample_count)
                        .build(),
                );
            }

            if let Some(attachment) = &pass.depth_stencil_resolve_attachment {
                attachments.push(
                    vk::AttachmentDescription2::builder()
                        .initial_layout(match attachment.load_op {
                            LoadOp::Load => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                            LoadOp::DontCare => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                            LoadOp::Clear(_) => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                        })
                        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .load_op(crate::util::to_vk_load_op(attachment.load_op))
                        .store_op(crate::util::to_vk_store_op(attachment.store_op))
                        .format(attachment.dst.internal().format)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .build(),
                );
            }

            // Link with attachment references
            let mut attachment_refs = Vec::with_capacity(pass.color_attachments.len());
            for (i, _) in pass.color_attachments.iter().enumerate() {
                attachment_refs.push(
                    vk::AttachmentReference2::builder()
                        .attachment(i as u32)
                        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .build(),
                );
            }

            let mut resolve_attachment_refs: Vec<_> = (0..pass.color_attachments.len())
                .map(|_| {
                    vk::AttachmentReference2::builder()
                        .attachment(vk::ATTACHMENT_UNUSED)
                        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .build()
                })
                .collect();

            for (i, attachment) in pass.color_resolve_attachments.iter().enumerate() {
                resolve_attachment_refs[attachment.src].attachment =
                    (pass.color_attachments.len() + i) as u32;
            }

            // Single subpass
            let depth_attachment;
            let depth_resolve_attachment;
            let mut depth_resolve;

            let subpass = vk::SubpassDescription2::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&attachment_refs)
                .resolve_attachments(&resolve_attachment_refs);

            let subpass = if pass.depth_stencil_attachment.is_some() {
                depth_attachment = vk::AttachmentReference2::builder()
                    .attachment(
                        (pass.color_attachments.len() + pass.color_resolve_attachments.len())
                            as u32,
                    )
                    .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .build();
                subpass.depth_stencil_attachment(&depth_attachment)
            } else {
                subpass
            };

            if let Some(resolve) = &pass.depth_stencil_resolve_attachment {
                depth_resolve_attachment = vk::AttachmentReference2::builder()
                    .attachment(
                        (pass.color_attachments.len() + pass.color_resolve_attachments.len() + 1)
                            as u32,
                    )
                    .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .build();

                depth_resolve = vk::SubpassDescriptionDepthStencilResolve::builder()
                    .depth_resolve_mode(crate::util::to_vk_resolve_mode(resolve.depth_resolve_mode))
                    .stencil_resolve_mode(crate::util::to_vk_resolve_mode(
                        resolve.stencil_resolve_mode,
                    ))
                    .depth_stencil_resolve_attachment(&depth_resolve_attachment)
                    .build();
            } else {
                depth_resolve = vk::SubpassDescriptionDepthStencilResolve::default();
            }

            let subpass = [subpass.push_next(&mut depth_resolve).build()];

            // Create the render pass
            unsafe {
                let create_info = vk::RenderPassCreateInfo2::builder()
                    .attachments(&attachments)
                    .subpasses(&subpass)
                    .build();

                VkRenderPass {
                    pass: device.create_render_pass2(&create_info, None).unwrap(),
                    samples: sample_count,
                }
            }
        })
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        for pass in self.passes.iter() {
            device.destroy_render_pass(pass.value().pass, None);
        }
    }
}

impl FramebufferCache {
    /// Given an ordered set of images and a render pass, produces a framebuffer.
    pub fn get(
        &self,
        device: &ash::Device,
        render_pass: vk::RenderPass,
        images: Vec<vk::ImageView>,
        extent: vk::Extent2D,
    ) -> vk::Framebuffer {
        // Get the framebuffers of the pass
        let mut framebuffers =
            self.pass_to_framebuffers
                .entry(render_pass)
                .or_insert(Framebuffers {
                    pass: render_pass,
                    framebuffers: HashMap::default(),
                });

        // Associate images to pass
        for image in &images {
            let mut passes = self.image_to_pass.entry(*image).or_default();
            passes.insert(render_pass);
        }

        // Get the framebuffer
        framebuffers.get(device, images, extent)
    }

    /// Call when an image is destroyed so framebuffers can be cleaned up.
    pub unsafe fn view_destroyed(&self, device: &ash::Device, image: vk::ImageView) {
        // Get the associated passes
        let passes = match self.image_to_pass.get(&image) {
            Some(passes) => passes,
            None => return,
        };

        // Loop over every pass and signal each one that the view is destroyed
        for pass in passes.value() {
            let mut framebuffers = match self.pass_to_framebuffers.get_mut(pass) {
                Some(framebuffers) => framebuffers,
                None => continue,
            };
            framebuffers.view_destroyed(device, image);
        }
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        for entry in self.pass_to_framebuffers.iter() {
            entry.release(device);
        }
    }
}

impl Framebuffers {
    #[inline(always)]
    pub fn get(
        &mut self,
        device: &ash::Device,
        images: Vec<vk::ImageView>,
        extent: vk::Extent2D,
    ) -> vk::Framebuffer {
        match self.framebuffers.get(&images) {
            Some(framebuffer) => *framebuffer,
            None => {
                let framebuffer = unsafe {
                    let create_info = vk::FramebufferCreateInfo::builder()
                        .attachments(&images)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1)
                        .render_pass(self.pass)
                        .build();

                    device.create_framebuffer(&create_info, None).unwrap()
                };
                self.framebuffers.insert(images, framebuffer);
                framebuffer
            }
        }
    }

    /// Call when an image is destroyed so framebuffers can be cleaned up.
    pub unsafe fn view_destroyed(&mut self, device: &ash::Device, image: vk::ImageView) {
        // TODO: Could probably make this faster
        let mut to_remove = Vec::default();
        for (key, value) in &self.framebuffers {
            if key.contains(&image) {
                // Destroy the framebuffer
                device.destroy_framebuffer(*value, None);

                // Signal to remove
                to_remove.push(key.clone());
            }
        }

        // Remove indicated
        for to_remove in to_remove {
            self.framebuffers.remove(&to_remove);
        }
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        for framebuffer in self.framebuffers.values() {
            device.destroy_framebuffer(*framebuffer, None);
        }
    }
}

impl VkRenderPassDescriptor {
    pub fn from_descriptor(
        descriptor: &RenderPassDescriptor<crate::VulkanBackend>,
    ) -> VkRenderPassDescriptor {
        let mut out = VkRenderPassDescriptor::default();
        for attachment in &descriptor.color_attachments {
            let (image_format, initial_layout, final_layout) = match &attachment.source {
                ColorAttachmentSource::SurfaceImage(image) => (
                    image.internal().format(),
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::UNDEFINED,
                        LoadOp::Load => vk::ImageLayout::PRESENT_SRC_KHR,
                        LoadOp::Clear(_) => vk::ImageLayout::UNDEFINED,
                    },
                    vk::ImageLayout::PRESENT_SRC_KHR,
                ),
                ColorAttachmentSource::Texture { texture, .. } => (
                    texture.internal().format,
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Clear(_) => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    },
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                ColorAttachmentSource::CubeMap { cube_map, .. } => (
                    cube_map.internal().format,
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Clear(_) => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    },
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
            };

            out.color_attachments.push(VkAttachment {
                image_format,
                initial_layout,
                final_layout,
                load_op: crate::util::to_vk_load_op(attachment.load_op),
                store_op: crate::util::to_vk_store_op(attachment.store_op),
                resolve_src: 0,
            });
        }

        for attachment in &descriptor.color_resolve_attachments {
            let (image_format, initial_layout, final_layout) = match &attachment.dst {
                ColorAttachmentSource::SurfaceImage(image) => (
                    image.internal().format(),
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::UNDEFINED,
                        LoadOp::Load => vk::ImageLayout::PRESENT_SRC_KHR,
                        LoadOp::Clear(_) => vk::ImageLayout::UNDEFINED,
                    },
                    vk::ImageLayout::PRESENT_SRC_KHR,
                ),
                ColorAttachmentSource::Texture { texture, .. } => (
                    texture.internal().format,
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Clear(_) => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    },
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                ColorAttachmentSource::CubeMap { cube_map, .. } => (
                    cube_map.internal().format,
                    match &attachment.load_op {
                        LoadOp::DontCare => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Load => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        LoadOp::Clear(_) => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    },
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
            };

            out.color_resolve_attachments.push(VkAttachment {
                image_format,
                initial_layout,
                final_layout,
                load_op: crate::util::to_vk_load_op(attachment.load_op),
                store_op: crate::util::to_vk_store_op(attachment.store_op),
                resolve_src: attachment.src,
            });
        }

        if let Some(attachment) = &descriptor.depth_stencil_attachment {
            let initial_layout = match &attachment.load_op {
                LoadOp::DontCare => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                LoadOp::Load => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                LoadOp::Clear(_) => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            };

            out.depth_stencil_attachment = Some(VkAttachment {
                image_format: attachment.texture.internal().format,
                initial_layout,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                load_op: crate::util::to_vk_load_op(attachment.load_op),
                store_op: crate::util::to_vk_store_op(attachment.store_op),
                resolve_src: 0,
            })
        }

        if let Some(attachment) = &descriptor.depth_stencil_resolve_attachment {
            let initial_layout = match &attachment.load_op {
                LoadOp::DontCare => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                LoadOp::Load => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                LoadOp::Clear(_) => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            };

            out.depth_stencil_resolve_attachment = Some(VkAttachment {
                image_format: attachment.dst.internal().format,
                initial_layout,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                load_op: crate::util::to_vk_load_op(attachment.load_op),
                store_op: crate::util::to_vk_store_op(attachment.store_op),
                resolve_src: 0,
            })
        }

        out
    }
}
