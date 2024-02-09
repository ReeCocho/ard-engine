use std::{collections::hash_map::Iter, hash::Hash};

use api::types::{CubeFace, QueueType, SharingMode};
use arrayvec::ArrayVec;
use ash::vk::{self, ImageLayout};
use rustc_hash::FxHashMap;

use crate::QueueFamilyIndices;

use super::{fast_int_hasher::FIHashMap, id_gen::ResourceId};

/// This keeps track of when and which queues buffers and images are used in. Additionally, it
/// keeps track of the layouts of images on a per-mip level.
#[derive(Default)]
pub(crate) struct GlobalResourceUsage {
    sets: Vec<GlobalSetUsage>,
    // First dim is buffer ID. Second is array element.
    buffers: Vec<Vec<GlobalBufferUsage>>,
    // First dim is image ID. Second is array element. Third is mip level.
    images: Vec<Vec<Vec<GlobalImageUsage>>>,
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(crate) struct BufferRegion {
    pub id: ResourceId,
    pub array_elem: u32,
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(crate) struct ImageRegion {
    pub id: ResourceId,
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
    usages: Vec<UsedSubResource>,
    queues: FIHashMap<QueueType, vk::PipelineStageFlags>,
}

#[derive(Default)]
pub(crate) struct PipelineTrackerScratchSpace {
    usages: Vec<UsedSubResource>,
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
        id: ResourceId,
    },
    Buffer {
        buffer: vk::Buffer,
        id: ResourceId,
        aligned_size: usize,
        array_elem: u32,
        sharing: SharingMode,
    },
    Texture {
        texture: vk::Image,
        id: ResourceId,
        aspect_mask: vk::ImageAspectFlags,
        array_elem: u32,
        base_mip_level: u32,
        mip_count: u32,
        sharing: SharingMode,
    },
    CubeMap {
        cube_map: vk::Image,
        id: ResourceId,
        aspect_mask: vk::ImageAspectFlags,
        array_elem: u32,
        mip_level: u32,
        sharing: SharingMode,
    },
    CubeFace {
        cube_map: vk::Image,
        id: ResourceId,
        aspect_mask: vk::ImageAspectFlags,
        array_elem: u32,
        face: CubeFace,
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

#[derive(Default, Copy, Clone)]
struct GlobalSetUsage {
    queue: Option<QueueUsage>,
}

#[derive(Default, Copy, Clone)]
struct GlobalBufferUsage {
    queue: Option<QueueUsage>,
    sub: Option<SubResourceUsage>,
}

#[derive(Default, Clone, Copy)]
struct GlobalImageUsage {
    queue: Option<QueueUsage>,
    layout: vk::ImageLayout,
    sub: Option<SubResourceUsage>,
}

#[derive(Copy, Clone)]
enum UsedSubResource {
    Set(ResourceId),
    Buffer {
        id: ResourceId,
        array_elem: u32,
    },
    Image {
        id: ResourceId,
        array_elem: u32,
        mip: u32,
    },
}

impl GlobalResourceUsage {
    #[inline(always)]
    pub fn get_layout(&self, region: &ImageRegion) -> vk::ImageLayout {
        self.images
            .get(region.id.as_idx())
            .and_then(|arrs| arrs.get(region.array_elem as usize))
            .and_then(|mips| mips.get(region.mip_level as usize))
            .and_then(|usage| Some(usage.layout))
            .unwrap_or(vk::ImageLayout::UNDEFINED)
    }

    #[inline(always)]
    pub fn register_set(
        &mut self,
        id: ResourceId,
        usage: Option<&QueueUsage>,
    ) -> Option<QueueUsage> {
        let idx = id.as_idx();

        match self.sets.get_mut(idx) {
            Some(old_usage) => {
                let old = old_usage.queue.take();
                old_usage.queue = usage.copied();
                old
            }
            None => {
                self.sets.resize(idx + 1, GlobalSetUsage::default());
                self.sets[idx].queue = usage.copied();
                None
            }
        }
    }

    pub fn register_buffer(
        &mut self,
        region: &BufferRegion,
        usage: Option<&QueueUsage>,
    ) -> Option<QueueUsage> {
        let idx = region.id.as_idx();
        let array_elem = region.array_elem as usize;

        match self.buffers.get_mut(idx) {
            Some(buffer_usages) => {
                // Expand if array element is OOB
                match buffer_usages.get_mut(array_elem) {
                    Some(old_usage) => {
                        let old = old_usage.queue.take();
                        old_usage.queue = usage.copied();
                        old
                    }
                    None => {
                        buffer_usages.resize(array_elem + 1, GlobalBufferUsage::default());
                        buffer_usages[array_elem].queue = usage.copied();
                        None
                    }
                }
            }
            // Buffer was not in the global usage region, so we must expand
            None => {
                // Expand buffers to fit the new region
                self.buffers.resize(idx + 1, Vec::default());
                self.buffers[idx].resize(array_elem + 1, GlobalBufferUsage::default());
                self.buffers[idx][array_elem].queue = usage.copied();
                None
            }
        }
    }

    #[inline(always)]
    pub fn register_image(
        &mut self,
        region: &ImageRegion,
        usage: &QueueUsage,
        layout: vk::ImageLayout,
    ) -> (Option<QueueUsage>, vk::ImageLayout) {
        let mut ret_usage = None;
        let mut ret_layout = vk::ImageLayout::UNDEFINED;
        self.register_images(
            region.id,
            region.array_elem as usize,
            region.mip_level as usize,
            1,
            usage,
            layout,
            |(_, old_usage, old_layout)| {
                ret_usage = old_usage;
                ret_layout = old_layout;
            },
        );
        (ret_usage, ret_layout)
    }

    #[inline(always)]
    pub fn register_images<F>(
        &mut self,
        id: ResourceId,
        array_elem: usize,
        base_mip: usize,
        mip_count: usize,
        usage: &QueueUsage,
        layout: vk::ImageLayout,
        mut on_set: F,
    ) where
        F: FnMut((usize, Option<QueueUsage>, ImageLayout)) -> (),
    {
        let idx = id.as_idx();
        let req_mip_range = base_mip + mip_count;

        let mip_range = match self.images.get_mut(idx) {
            Some(image_array_elems) => {
                // Expand if array element is OOB
                match image_array_elems.get_mut(array_elem) {
                    Some(image_mips) => {
                        if image_mips.len() < base_mip + mip_count {
                            image_mips.resize(req_mip_range, GlobalImageUsage::default());
                        }
                        &mut self.images[idx][array_elem][base_mip..(base_mip + mip_count)]
                    }
                    None => {
                        image_array_elems.resize(array_elem + 1, Vec::default());
                        image_array_elems[array_elem]
                            .resize(req_mip_range, GlobalImageUsage::default());
                        &mut self.images[idx][array_elem][base_mip..(base_mip + mip_count)]
                    }
                }
            }
            None => {
                self.images.resize(idx + 1, Vec::default());
                self.images[idx].resize(array_elem + 1, Vec::default());
                self.images[idx][array_elem].resize(req_mip_range, GlobalImageUsage::default());
                &mut self.images[idx][array_elem][base_mip..(base_mip + mip_count)]
            }
        };

        for (i, global_usage) in mip_range.iter_mut().enumerate() {
            let old = global_usage.queue.take();
            let old_layout = global_usage.layout;

            global_usage.queue = Some(*usage);
            global_usage.layout = layout;

            on_set((base_mip + i, old, old_layout));
        }
    }

    #[inline(always)]
    pub fn set_layout(&mut self, region: &ImageRegion, new_layout: vk::ImageLayout) {
        if let Some(array_elems) = self.images.get_mut(region.id.as_idx()) {
            if let Some(mips) = array_elems.get_mut(region.array_elem as usize) {
                if let Some(usage) = mips.get_mut(region.mip_level as usize) {
                    usage.layout = new_layout;
                }
            }
        }
    }

    #[inline(always)]
    pub fn remove_buffer(&mut self, id: ResourceId) {
        if let Some(usages) = self.buffers.get_mut(id.as_idx()) {
            usages.clear();
        }
    }

    #[inline(always)]
    pub fn remove_image(&mut self, id: ResourceId) {
        if let Some(array_elems) = self.images.get_mut(id.as_idx()) {
            array_elems.iter_mut().for_each(|e| e.clear());
        }
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
        puffin::profile_function!();

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
        let mut sub_resources: ArrayVec<
            (
                Option<QueueUsage>,
                vk::ImageLayout,
                SharingMode,
                UsedSubResource,
            ),
            // NOTE: This number was selected because it's (probably) the most mip levels a
            // texture will ever have (for a 16K texture). If it's the future and we're using
            // bigger textures for some stupid reason, change this.
            14,
        > = ArrayVec::default();

        for (resource, mut usage) in scope.usages.drain() {
            sub_resources.clear();

            // Check the global tracker to see if we need to wait on certain queues or if we need
            // a layout transition.
            let resc_usage = QueueUsage {
                queue: self.queue_ty,
                timeline_value: self.next_value,
                reaquire: None,
            };

            match &resource {
                SubResource::Set { id, .. } => {
                    sub_resources.push((
                        self.global.register_set(*id, Some(&resc_usage)),
                        vk::ImageLayout::UNDEFINED,
                        SharingMode::Concurrent,
                        UsedSubResource::Set(*id),
                    ));
                }
                SubResource::Buffer {
                    id,
                    array_elem,
                    sharing,
                    ..
                } => {
                    sub_resources.push((
                        self.global.register_buffer(
                            &BufferRegion {
                                id: *id,
                                array_elem: *array_elem,
                            },
                            Some(&resc_usage),
                        ),
                        vk::ImageLayout::UNDEFINED,
                        *sharing,
                        UsedSubResource::Buffer {
                            id: *id,
                            array_elem: *array_elem,
                        },
                    ));
                }
                SubResource::Texture {
                    id,
                    array_elem,
                    base_mip_level,
                    mip_count,
                    sharing,
                    ..
                } => {
                    self.global.register_images(
                        *id,
                        *array_elem as usize,
                        *base_mip_level as usize,
                        *mip_count as usize,
                        &resc_usage,
                        usage.layout,
                        |(mip_level, usage, layout)| {
                            sub_resources.push((
                                usage,
                                layout,
                                *sharing,
                                UsedSubResource::Image {
                                    id: *id,
                                    array_elem: *array_elem,
                                    mip: mip_level as u32,
                                },
                            ));
                        },
                    );
                }
                SubResource::CubeMap {
                    id,
                    array_elem,
                    mip_level,
                    sharing,
                    ..
                } => {
                    for i in 0..6 {
                        let (usage, layout) = self.global.register_image(
                            &ImageRegion {
                                id: *id,
                                array_elem: (*array_elem * 6) + i,
                                mip_level: *mip_level,
                            },
                            &resc_usage,
                            usage.layout,
                        );
                        sub_resources.push((
                            usage,
                            layout,
                            *sharing,
                            UsedSubResource::Image {
                                id: *id,
                                array_elem: (*array_elem * 6) + i,
                                mip: *mip_level,
                            },
                        ));
                    }
                }
                SubResource::CubeFace {
                    id,
                    array_elem,
                    face,
                    mip_level,
                    sharing,
                    ..
                } => {
                    let (usage, layout) = self.global.register_image(
                        &ImageRegion {
                            id: *id,
                            array_elem: crate::cube_map::CubeMap::to_array_elem(
                                *array_elem as usize,
                                *face,
                            ) as u32,
                            mip_level: *mip_level,
                        },
                        &resc_usage,
                        usage.layout,
                    );

                    sub_resources.push((
                        usage,
                        layout,
                        *sharing,
                        UsedSubResource::Image {
                            id: *id,
                            array_elem: crate::cube_map::CubeMap::to_array_elem(
                                *array_elem as usize,
                                *face,
                            ) as u32,
                            mip: *mip_level,
                        },
                    ));
                }
            };

            for (old_queue_usage, old_layout, sharing, sub_resc) in sub_resources.drain(..) {
                // If we have mismatching layouts, a couple things happen:
                // 1. A barrier is needed
                // 2. We need the transfer stage
                let mut needs_barrier = false;
                let mut src_qfi;
                let mut dst_qfi = self.qfi.to_index(self.queue_ty);

                if old_layout != usage.layout {
                    needs_barrier = true;
                    usage.access |=
                        vk::AccessFlags::TRANSFER_WRITE | vk::AccessFlags::TRANSFER_READ;
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

                let mut dummy = None;
                let old_sub_usage = match sub_resc {
                    UsedSubResource::Set(_) => &mut dummy,
                    UsedSubResource::Buffer { id, array_elem } => {
                        &mut self.global.buffers[id.as_idx()][array_elem as usize].sub
                    }
                    UsedSubResource::Image {
                        id,
                        array_elem,
                        mip,
                    } => {
                        &mut self.global.images[id.as_idx()][array_elem as usize][mip as usize].sub
                    }
                };

                let (src_access, src_stage) = match old_sub_usage {
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

                match old_sub_usage {
                    Some(old) => *old = usage,
                    None => {
                        self.usages.push(sub_resc);
                        *old_sub_usage = Some(usage);
                    }
                }

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
                            aspect_mask,
                            ..
                        } => {
                            let mip_level = match sub_resc {
                                UsedSubResource::Image { mip, .. } => mip,
                                _ => unreachable!("subresource should always be an image"),
                            };

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
                        SubResource::CubeFace {
                            cube_map,
                            aspect_mask,
                            array_elem,
                            face,
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
                                        base_array_layer: crate::cube_map::CubeMap::to_array_elem(
                                            array_elem as usize,
                                            face,
                                        )
                                            as u32,
                                        layer_count: 1,
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
        // Reset global usages
        self.usages.drain(..).for_each(|sub_resc| match sub_resc {
            UsedSubResource::Set(_) => {}
            UsedSubResource::Buffer { id, array_elem } => {
                self.global.buffers[id.as_idx()][array_elem as usize].sub = None
            }
            UsedSubResource::Image {
                id,
                array_elem,
                mip,
            } => self.global.images[id.as_idx()][array_elem as usize][mip as usize].sub = None,
        });

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
