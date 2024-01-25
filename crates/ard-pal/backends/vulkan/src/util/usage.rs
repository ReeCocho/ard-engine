use std::{collections::hash_map::Iter, hash::Hash};

use api::types::{QueueType, SharingMode};
use ash::vk::{self, ImageLayout};
use rustc_hash::FxHashMap;

use crate::QueueFamilyIndices;

use super::fast_int_hasher::FIHashMap;

/// This keeps track of when and which queues buffers and images are used in. Additionally, it
/// keeps track of the layouts of images on a per-mip level.
#[derive(Default)]
pub(crate) struct GlobalResourceUsage {
    sets: FIHashMap<vk::DescriptorSet, QueueUsage>,
    buffers: FxHashMap<BufferRegion, QueueUsage>,
    images: FxHashMap<ImageRegion, (QueueUsage, vk::ImageLayout)>,
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(crate) struct BufferRegion {
    pub buffer: vk::Buffer,
    pub array_elem: u32,
}

#[derive(Copy, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub(crate) struct ImageRegion {
    pub image: vk::Image,
    pub array_elem: u32,
    pub mip_level: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct QueueUsage {
    /// Which queue is using the resource.
    pub queue: QueueType,
    /// The timeline value the `queue` must reach before the resource is released.
    pub timeline_value: u64,
    /// If the resource is being released, indicates which queue must reaquire the resource.
    pub reaquire: Option<QueueType>,
}

pub(crate) struct PipelineTracker<'a> {
    global: &'a mut GlobalResourceUsage,
    qfi: &'a QueueFamilyIndices,
    queue_ty: QueueType,
    next_value: u64,
    usages: FxHashMap<SubResource, SubResourceUsage>,
    queues: FIHashMap<QueueType, vk::PipelineStageFlags>,
}

#[derive(Default)]
pub(crate) struct PipelineTrackerScratchSpace {
    usages: FxHashMap<SubResource, SubResourceUsage>,
    queues: FIHashMap<QueueType, vk::PipelineStageFlags>,
}

#[derive(Default)]
pub(crate) struct UsageScope {
    usages: FxHashMap<SubResource, SubResourceUsage>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SubResourceUsage {
    pub access: vk::AccessFlags,
    pub stage: vk::PipelineStageFlags,
    /// Unused by buffers.
    pub layout: vk::ImageLayout,
}

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub(crate) enum SubResource {
    Set {
        set: vk::DescriptorSet,
    },
    Buffer {
        buffer: vk::Buffer,
        aligned_size: usize,
        array_elem: u32,
        sharing: SharingMode,
    },
    Texture {
        texture: vk::Image,
        aspect_mask: vk::ImageAspectFlags,
        array_elem: u32,
        mip_level: u32,
        sharing: SharingMode,
    },
    CubeMap {
        cube_map: vk::Image,
        aspect_mask: vk::ImageAspectFlags,
        array_elem: u32,
        mip_level: u32,
        sharing: SharingMode,
    },
}

#[derive(Default)]
pub(crate) struct PipelineBarrier {
    pub src_stage: vk::PipelineStageFlags,
    pub dst_stage: vk::PipelineStageFlags,
    pub buffer_barriers: Vec<vk::BufferMemoryBarrier>,
    pub image_barriers: Vec<vk::ImageMemoryBarrier>,
}

impl GlobalResourceUsage {
    #[inline(always)]
    pub fn get_layout(&self, region: &ImageRegion) -> vk::ImageLayout {
        *self
            .images
            .get(region)
            .map(|(_, layout)| layout)
            .unwrap_or(&vk::ImageLayout::UNDEFINED)
    }

    #[inline(always)]
    pub fn register_set(
        &mut self,
        set: vk::DescriptorSet,
        usage: Option<&QueueUsage>,
    ) -> Option<QueueUsage> {
        match usage {
            Some(usage) => self.sets.insert(set, *usage),
            None => self.sets.remove(&set),
        }
    }

    #[inline(always)]
    pub fn register_buffer(
        &mut self,
        region: &BufferRegion,
        usage: Option<&QueueUsage>,
    ) -> Option<QueueUsage> {
        match usage {
            Some(usage) => self.buffers.insert(*region, *usage),
            None => self.buffers.remove(region),
        }
    }

    #[inline(always)]
    pub fn register_image(
        &mut self,
        region: &ImageRegion,
        usage: &QueueUsage,
        layout: vk::ImageLayout,
    ) -> (Option<QueueUsage>, vk::ImageLayout) {
        self.images
            .insert(*region, (*usage, layout))
            .map(|(usage, layout)| (Some(usage), layout))
            .unwrap_or((None, ImageLayout::UNDEFINED))
    }

    #[inline(always)]
    pub fn set_layout(&mut self, region: &ImageRegion, new_layout: vk::ImageLayout) {
        if let Some((_, layout)) = self.images.get_mut(region) {
            *layout = new_layout;
        }
    }

    #[inline(always)]
    pub fn remove_buffer(&mut self, region: &BufferRegion) {
        self.buffers.remove(&region);
    }

    #[inline(always)]
    pub fn remove_image(&mut self, region: &ImageRegion) {
        self.images.remove(&region);
    }
}

impl<'a> PipelineTracker<'a> {
    #[inline(always)]
    pub fn new(
        global: &'a mut GlobalResourceUsage,
        qfi: &'a QueueFamilyIndices,
        queue_ty: QueueType,
        next_value: u64,
        scratch: PipelineTrackerScratchSpace,
    ) -> Self {
        Self {
            global,
            qfi,
            queue_ty,
            next_value,
            usages: scratch.usages,
            queues: scratch.queues,
        }
    }

    pub fn submit(&mut self, scope: &mut UsageScope) -> Option<PipelineBarrier> {
        let read_accesses: vk::AccessFlags = vk::AccessFlags::MEMORY_READ
            | vk::AccessFlags::SHADER_READ
            | vk::AccessFlags::UNIFORM_READ
            | vk::AccessFlags::TRANSFER_READ
            | vk::AccessFlags::COLOR_ATTACHMENT_READ
            | vk::AccessFlags::INDIRECT_COMMAND_READ
            | vk::AccessFlags::VERTEX_ATTRIBUTE_READ
            | vk::AccessFlags::INDEX_READ
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;

        let mut barrier = PipelineBarrier::default();

        // Keeps track of which image subresources need memor barriers and/or layout transitions
        let mut image_barriers =
            FxHashMap::<(vk::Image, u32, u32), vk::ImageMemoryBarrier>::default();

        // Keeps track of which buffers need memory barriers
        let mut buffer_barriers =
            FxHashMap::<(vk::Buffer, u32), vk::BufferMemoryBarrier>::default();

        // Analyze each usage
        for (resource, mut usage) in scope.usages.drain() {
            // Check the global tracker to see if we need to wait on certain queues or if we need
            // a layout transition.
            let resc_usage = QueueUsage {
                queue: self.queue_ty,
                timeline_value: self.next_value,
                reaquire: None,
            };
            let (old_queue_usage, old_layout, sharing) = match &resource {
                SubResource::Set { set, .. } => (
                    self.global.register_set(*set, Some(&resc_usage)),
                    vk::ImageLayout::UNDEFINED,
                    SharingMode::Concurrent,
                ),
                SubResource::Buffer {
                    buffer,
                    array_elem,
                    sharing,
                    ..
                } => (
                    self.global.register_buffer(
                        &BufferRegion {
                            buffer: *buffer,
                            array_elem: *array_elem,
                        },
                        Some(&resc_usage),
                    ),
                    vk::ImageLayout::UNDEFINED,
                    *sharing,
                ),
                SubResource::Texture {
                    texture,
                    array_elem,
                    mip_level,
                    sharing,
                    ..
                } => {
                    let (usage, layout) = self.global.register_image(
                        &ImageRegion {
                            image: *texture,
                            array_elem: *array_elem,
                            mip_level: *mip_level,
                        },
                        &resc_usage,
                        usage.layout,
                    );

                    (usage, layout, *sharing)
                }
                SubResource::CubeMap {
                    cube_map,
                    array_elem,
                    mip_level,
                    sharing,
                    ..
                } => {
                    let (usage, layout) = self.global.register_image(
                        &ImageRegion {
                            image: *cube_map,
                            array_elem: *array_elem,
                            mip_level: *mip_level,
                        },
                        &resc_usage,
                        usage.layout,
                    );

                    (usage, layout, *sharing)
                }
            };

            // If we have mismatching layouts, a couple things happen:
            // 1. A barrier is needed
            // 2. We need the transfer stage
            let mut needs_barrier = false;
            let mut src_qfi;
            let mut dst_qfi = self.qfi.to_index(self.queue_ty);

            if old_layout != usage.layout {
                needs_barrier = true;
                usage.access |= vk::AccessFlags::TRANSFER_WRITE | vk::AccessFlags::TRANSFER_READ;
                usage.stage |= vk::PipelineStageFlags::TRANSFER;
            }

            // Check if this resource was last used by a queue other than us
            if let Some(old_queue_usage) = old_queue_usage {
                if old_queue_usage.queue != self.queue_ty {
                    // Mark which stage needs to wait on the previous queue
                    let entry = self
                        .queues
                        .entry(old_queue_usage.queue)
                        .or_insert(vk::PipelineStageFlags::empty());
                    *entry |= usage.stage;
                }

                src_qfi = self.qfi.to_index(old_queue_usage.queue);
            } else {
                src_qfi = vk::QUEUE_FAMILY_EXTERNAL;
            }

            // If the families are the same, or we don't need exclusive access, ownership transfer
            // is not needed
            if src_qfi == dst_qfi || sharing != SharingMode::Exclusive {
                src_qfi = vk::QUEUE_FAMILY_IGNORED;
                dst_qfi = vk::QUEUE_FAMILY_IGNORED;
            } else {
                needs_barrier = true;
            }

            let (src_access, src_stage) = match self.usages.get(&resource) {
                Some(old) => {
                    // Anything other than read-after-read requires a barrier
                    if !((read_accesses | old.access == read_accesses)
                        && (read_accesses | usage.access == read_accesses))
                    {
                        needs_barrier = true;
                    }
                    (old.access, old.stage)
                }
                // If there was no previous usage, no barrier is needed
                None => (vk::AccessFlags::NONE, vk::PipelineStageFlags::TOP_OF_PIPE),
            };
            self.usages.insert(resource, usage);

            if needs_barrier {
                match resource {
                    SubResource::Buffer {
                        buffer,
                        array_elem,
                        aligned_size,
                        ..
                    } => {
                        buffer_barriers.insert(
                            (buffer, array_elem),
                            vk::BufferMemoryBarrier::builder()
                                .src_access_mask(src_access)
                                .dst_access_mask(usage.access)
                                .src_queue_family_index(src_qfi)
                                .dst_queue_family_index(dst_qfi)
                                .buffer(buffer)
                                .offset((aligned_size * array_elem as usize) as vk::DeviceSize)
                                .size(aligned_size as vk::DeviceSize)
                                .build(),
                        );
                    }
                    SubResource::Texture {
                        texture,
                        array_elem,
                        mip_level,
                        aspect_mask,
                        ..
                    } => {
                        image_barriers.insert(
                            (texture, array_elem, mip_level),
                            vk::ImageMemoryBarrier::builder()
                                .src_access_mask(src_access)
                                .dst_access_mask(usage.access)
                                .old_layout(old_layout)
                                .new_layout(usage.layout)
                                .src_queue_family_index(src_qfi)
                                .dst_queue_family_index(dst_qfi)
                                .image(texture)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask,
                                    base_mip_level: mip_level,
                                    level_count: 1,
                                    base_array_layer: array_elem,
                                    layer_count: 1,
                                })
                                .build(),
                        );
                    }
                    SubResource::CubeMap {
                        cube_map,
                        aspect_mask,
                        array_elem,
                        mip_level,
                        ..
                    } => {
                        image_barriers.insert(
                            (cube_map, array_elem, mip_level),
                            vk::ImageMemoryBarrier::builder()
                                .src_access_mask(src_access)
                                .dst_access_mask(usage.access)
                                .old_layout(old_layout)
                                .new_layout(usage.layout)
                                .src_queue_family_index(src_qfi)
                                .dst_queue_family_index(dst_qfi)
                                .image(cube_map)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask,
                                    base_mip_level: mip_level,
                                    level_count: 1,
                                    base_array_layer: array_elem * 6,
                                    layer_count: 6,
                                })
                                .build(),
                        );
                    }
                    _ => {}
                }

                // Update barrier with stages
                barrier.dst_stage |= usage.stage;
                barrier.src_stage |= src_stage;
            }
        }

        scope.reset();

        // We only need a barrier if we have registered buffer/image barriers
        if !image_barriers.is_empty() || !buffer_barriers.is_empty() {
            barrier.image_barriers = image_barriers.into_values().collect();
            barrier.buffer_barriers = buffer_barriers.into_values().collect();
            Some(barrier)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn wait_queues(&self) -> Iter<'_, QueueType, vk::PipelineStageFlags> {
        self.queues.iter()
    }

    #[inline(always)]
    pub fn into_scratch_space(mut self) -> PipelineTrackerScratchSpace {
        self.usages.clear();
        self.queues.clear();

        PipelineTrackerScratchSpace {
            usages: self.usages,
            queues: self.queues,
        }
    }
}

impl UsageScope {
    #[inline(always)]
    pub fn reset(&mut self) {
        self.usages.clear();
    }

    #[inline(always)]
    pub fn use_resource(&mut self, subresource: &SubResource, usage: &SubResourceUsage) {
        let entry = self.usages.entry(*subresource).or_default();
        let need_general =
            entry.layout == vk::ImageLayout::GENERAL || usage.layout == vk::ImageLayout::GENERAL;

        assert!(
            // Don't care if the previous layout is undefined
            entry.layout == vk::ImageLayout::UNDEFINED ||
            // Don't care if the layouts match
            entry.layout == usage.layout ||
            // Don't care if one of the layouts is general
            need_general,
            "an image can only have one layout per scope"
        );

        entry.layout = if need_general {
            vk::ImageLayout::GENERAL
        } else {
            usage.layout
        };
        entry.access |= usage.access;
        entry.stage |= usage.stage;
    }
}

impl PipelineBarrier {
    #[inline(always)]
    pub unsafe fn execute(&self, device: &ash::Device, command_buffer: vk::CommandBuffer) {
        device.cmd_pipeline_barrier(
            command_buffer,
            self.src_stage,
            self.dst_stage,
            vk::DependencyFlags::BY_REGION,
            &[],
            &self.buffer_barriers,
            &self.image_barriers,
        );
    }
}

/*
// SAFETY: As long as `ImageRegion` is 8 byte aligned and a size that is a multiple of 8
// bytes, this is safe.
unsafe impl Pod for ImageRegion {}
unsafe impl Zeroable for ImageRegion {}

impl PartialEq for ImageRegion {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            let our_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(self)).unsafe_unwrap();
            let their_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(other)).unsafe_unwrap();

            our_words.get_unchecked(0) == their_words.get_unchecked(0)
                && our_words.get_unchecked(1) == their_words.get_unchecked(1)
        }
    }
}

impl Hash for ImageRegion {
    #[inline(always)]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe {
            let our_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(self)).unsafe_unwrap();
            our_words.get_unchecked(0).hash(state);
            our_words.get_unchecked(1).hash(state);
        }
    }
}

// SAFETY: The discriminant of `SubResource` needs to be 8 bytes big, and the total size needs to
// be 32 bytes.
unsafe impl Pod for SubResource {}
unsafe impl Zeroable for SubResource {}

impl PartialEq for SubResource {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            let our_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(self)).unsafe_unwrap();
            let their_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(other)).unsafe_unwrap();

            std::mem::discriminant(self) == std::mem::discriminant(other)
                && our_words.get_unchecked(1) == their_words.get_unchecked(1)
                && our_words.get_unchecked(2) == their_words.get_unchecked(2)
                && our_words.get_unchecked(3) == their_words.get_unchecked(3)
        }
    }
}

impl Hash for SubResource {
    #[inline(always)]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe {
            let our_words =
                bytemuck::try_cast_slice::<_, u64>(core::slice::from_ref(self)).unsafe_unwrap();
            std::mem::discriminant(self).hash(state);
            our_words.get_unchecked(1).hash(state);
            our_words.get_unchecked(2).hash(state);
            our_words.get_unchecked(3).hash(state);
        }
    }
}
*/
