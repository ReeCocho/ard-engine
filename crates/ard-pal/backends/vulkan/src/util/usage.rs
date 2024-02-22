use api::types::{QueueType, SharingMode};
use ash::vk;

use super::id_gen::ResourceId;

#[derive(Default)]
pub(crate) struct GlobalResourceUsage {
    sets: Vec<GlobalSetUsage>,
    // First dim is buffer ID. Second is array element.
    buffers: Vec<Vec<InternalGlobalBufferUsage>>,
    // First dim is image ID. Second is array element. Third is mip level.
    images: Vec<Vec<Vec<InternalGlobalImageUsage>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct QueueUsage {
    pub queue: QueueType,
    pub timeline_value: u64,
    pub command_idx: usize,
    pub is_async: bool,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SubResourceUsage {
    pub access: vk::AccessFlags2,
    pub stage: vk::PipelineStageFlags2,
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
    pub base_mip_level: u32,
    pub mip_count: u32,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct GlobalSetUsage {
    pub queue: Option<QueueUsage>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct GlobalBufferUsage {
    pub queue: Option<QueueUsage>,
    pub sub_resource: SubResourceUsage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct GlobalImageUsage {
    pub queue: Option<QueueUsage>,
    pub sub_resource: SubResourceUsage,
    pub layout: vk::ImageLayout,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct InternalQueueUsage {
    pub queue: QueueType,
    pub timeline_value: u64,
    pub is_async: bool,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct InternalGlobalBufferUsage {
    queue: Option<InternalQueueUsage>,
    write_command: Option<usize>,
    write_sub_resource: SubResourceUsage,
    read_command: Option<usize>,
    read_sub_resource: SubResourceUsage,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct InternalGlobalImageUsage {
    queue: Option<InternalQueueUsage>,
    write_command: Option<usize>,
    write_sub_resource: SubResourceUsage,
    read_command: Option<usize>,
    read_sub_resource: SubResourceUsage,
    layout: vk::ImageLayout,
}

#[derive(Copy, Clone)]
pub(crate) enum PipelineBarrier {
    Memory(vk::MemoryBarrier2),
    Buffer(vk::BufferMemoryBarrier2),
    Image(vk::ImageMemoryBarrier2),
}

impl GlobalResourceUsage {
    #[inline(always)]
    pub fn get_set_queue_usage(&self, id: ResourceId) -> Option<&QueueUsage> {
        match self.sets.get(id.as_idx()) {
            Some(usage) => usage.queue.as_ref(),
            None => None,
        }
    }

    #[inline(always)]
    pub fn get_buffer_queue_usage(&self, region: &BufferRegion) -> Option<QueueUsage> {
        let array_elems = match self.buffers.get(region.id.as_idx()) {
            Some(elems) => elems,
            None => return None,
        };

        let queue_usage = match array_elems.get(region.array_elem as usize) {
            Some(usage) => usage.queue.as_ref(),
            None => None,
        };

        queue_usage.map(|q| QueueUsage {
            queue: q.queue,
            timeline_value: q.timeline_value,
            command_idx: usize::MAX,
            is_async: q.is_async,
        })
    }

    #[inline(always)]
    pub fn use_buffer(
        &mut self,
        region: &BufferRegion,
        new_usage: &GlobalBufferUsage,
    ) -> GlobalBufferUsage {
        let old_usage = self.get_buffer_entry(region);

        // If there was no previous queue usage, or the usage was a previous queue submit, we can
        // early out with a simple usage since a barrier is uneccesary.
        let no_barrier = match (old_usage.queue, new_usage.queue) {
            (Some(old), Some(new)) => {
                old.queue != new.queue || old.timeline_value < new.timeline_value
            }
            _ => true,
        };

        if no_barrier {
            // Construct our result usage.
            let res = GlobalBufferUsage {
                queue: old_usage.queue.map(|old| QueueUsage {
                    queue: old.queue,
                    timeline_value: old.timeline_value,
                    // NOTE: Since the previous usage was a different submit, the command index
                    // has no meaning anymore, so it's find to set it to a default.
                    command_idx: usize::MAX,
                    is_async: old.is_async,
                }),
                sub_resource: SubResourceUsage::default(),
            };

            // Update the old usage with the new values
            match new_usage.queue {
                Some(new_queue) => {
                    old_usage.queue = Some(new_queue.into());
                    if new_usage.sub_resource.is_write() {
                        old_usage.write_command = Some(new_queue.command_idx);
                        old_usage.write_sub_resource = new_usage.sub_resource;
                        old_usage.read_command = None;
                        old_usage.read_sub_resource = SubResourceUsage::default();
                    } else {
                        old_usage.read_command = Some(new_queue.command_idx);
                        old_usage.read_sub_resource = new_usage.sub_resource;
                        old_usage.write_command = None;
                        old_usage.write_sub_resource = SubResourceUsage::default();
                    }
                }
                None => *old_usage = InternalGlobalBufferUsage::default(),
            }

            return res;
        }

        // If the the new usage is a write, we need to sync with the most recent command.
        let old_queue_usage = old_usage.queue;
        let (sub_resource, command_idx) = if new_usage.sub_resource.is_write() {
            let (sub_resc, idx) = match (old_usage.read_command, old_usage.write_command) {
                (Some(r), Some(w)) => {
                    if r > w {
                        (old_usage.read_sub_resource, r)
                    } else {
                        (old_usage.write_sub_resource, w)
                    }
                }
                (Some(r), None) => (old_usage.read_sub_resource, r),
                (None, Some(w)) => (old_usage.write_sub_resource, w),
                (None, None) => {
                    unreachable!("there must always be a previous command if we passed early out")
                }
            };

            // Update write usage
            old_usage.queue = new_usage.queue.map(|queue| queue.into());
            old_usage.write_command = new_usage.queue.map(|queue| queue.command_idx);
            old_usage.write_sub_resource = new_usage.sub_resource;

            (sub_resc, idx)
        }
        // If the new usage is a read, we need to sync with the last write.
        else {
            let idx = old_usage.write_command.unwrap_or(usize::MAX);

            old_usage.queue = new_usage.queue.map(|queue| queue.into());
            old_usage.read_command = new_usage.queue.map(|queue| queue.command_idx);
            old_usage.read_sub_resource = new_usage.sub_resource;

            (old_usage.write_sub_resource, idx)
        };

        GlobalBufferUsage {
            queue: old_queue_usage.map(|queue| QueueUsage {
                queue: queue.queue,
                timeline_value: queue.timeline_value,
                command_idx,
                is_async: queue.is_async,
            }),
            sub_resource,
        }
    }

    #[inline(always)]
    pub fn use_image(
        &mut self,
        region: &ImageRegion,
        new_usage: &GlobalImageUsage,
        out_old_usages: &mut [GlobalImageUsage],
    ) {
        let old = self.get_image_entries(region);
        out_old_usages
            .iter_mut()
            .zip(old)
            .for_each(|(out, old_usage)| {
                let needs_layout_transition = new_usage.layout != vk::ImageLayout::UNDEFINED
                    && new_usage.layout != old_usage.layout;

                // If there was no previous queue usage, or the usage was a previous queue submit, we
                // can early out with a simple usage since a barrier is uneccesary.
                let no_barrier = match (old_usage.queue, new_usage.queue) {
                    (Some(old), Some(new)) => {
                        old.queue != new.queue || old.timeline_value < new.timeline_value
                    }
                    _ => true,
                };

                if no_barrier {
                    // Construct our result usage.
                    let res = GlobalImageUsage {
                        queue: old_usage.queue.map(|old| QueueUsage {
                            queue: old.queue,
                            timeline_value: old.timeline_value,
                            // NOTE: Since the previous usage was a different submit, the command index
                            // has no meaning anymore, so it's find to set it to a default.
                            command_idx: usize::MAX,
                            is_async: old.is_async,
                        }),
                        sub_resource: SubResourceUsage::default(),
                        layout: old_usage.layout,
                    };

                    // Update the old usage with the new values
                    match new_usage.queue {
                        Some(new_queue) => {
                            old_usage.queue = Some(new_queue.into());
                            if new_usage.sub_resource.is_write() {
                                old_usage.write_command = Some(new_queue.command_idx);
                                old_usage.write_sub_resource = new_usage.sub_resource;
                                old_usage.read_command = None;
                                old_usage.read_sub_resource = SubResourceUsage::default();
                            } else {
                                old_usage.read_command = Some(new_queue.command_idx);
                                old_usage.read_sub_resource = new_usage.sub_resource;
                                old_usage.write_command = None;
                                old_usage.write_sub_resource = SubResourceUsage::default();
                            }
                        }
                        None => {
                            old_usage.read_command = None;
                            old_usage.read_sub_resource = SubResourceUsage::default();
                            old_usage.write_command = None;
                            old_usage.write_sub_resource = SubResourceUsage::default();
                        }
                    }

                    // Update layout
                    if needs_layout_transition {
                        old_usage.layout = new_usage.layout;
                    }

                    *out = res;
                    return;
                }

                // If the the new usage is a write or we have mismatching layouts, we need to sync
                // with the most recent command.
                let old_layout = old_usage.layout;
                let old_queue_usage = old_usage.queue;
                old_usage.queue = new_usage.queue.map(|queue| queue.into());

                let (sub_resource, command_idx) = if new_usage.sub_resource.is_write()
                    || needs_layout_transition
                {
                    let (sub_resc, idx) = match (old_usage.read_command, old_usage.write_command) {
                        (Some(r), Some(w)) => {
                            if r > w {
                                (old_usage.read_sub_resource, r)
                            } else {
                                (old_usage.write_sub_resource, w)
                            }
                        }
                        (Some(r), None) => (old_usage.read_sub_resource, r),
                        (None, Some(w)) => (old_usage.write_sub_resource, w),
                        (None, None) => (old_usage.read_sub_resource, usize::MAX),
                    };

                    // Update write usage (or read, if this is just a layout transition)
                    if new_usage.sub_resource.is_write() {
                        old_usage.write_command = new_usage.queue.map(|queue| queue.command_idx);
                        old_usage.write_sub_resource = new_usage.sub_resource;
                    } else {
                        old_usage.read_command = new_usage.queue.map(|queue| queue.command_idx);
                        old_usage.read_sub_resource = new_usage.sub_resource;
                    }
                    old_usage.layout = new_usage.layout;

                    (sub_resc, idx)
                }
                // If the new usage is a read, we need to sync with the last write.
                else {
                    let idx = old_usage.write_command.unwrap_or(usize::MAX);

                    old_usage.read_command = new_usage.queue.map(|queue| queue.command_idx);
                    old_usage.read_sub_resource = new_usage.sub_resource;

                    (old_usage.write_sub_resource, idx)
                };

                *out = GlobalImageUsage {
                    queue: old_queue_usage.map(|queue| QueueUsage {
                        queue: queue.queue,
                        timeline_value: queue.timeline_value,
                        command_idx,
                        is_async: queue.is_async,
                    }),
                    sub_resource,
                    layout: old_layout,
                };
            });
    }

    #[inline(always)]
    pub fn set_image_layout(&mut self, region: &ImageRegion, new_layout: vk::ImageLayout) {
        if let Some(array_elems) = self.images.get_mut(region.id.as_idx()) {
            if let Some(mips) = array_elems.get_mut(region.array_elem as usize) {
                for i in region.base_mip_level..(region.base_mip_level + region.mip_count) {
                    if let Some(usage) = mips.get_mut(i as usize) {
                        usage.layout = new_layout;
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn use_set(&mut self, id: ResourceId, usage: &GlobalSetUsage) -> GlobalSetUsage {
        let old_usage = self.get_set_entry(id);
        std::mem::replace(old_usage, *usage)
    }

    #[inline(always)]
    pub fn remove_buffer(&mut self, id: ResourceId) {
        if let Some(elems) = self.buffers.get_mut(id.as_idx()) {
            elems.clear();
        }
    }

    #[inline(always)]
    pub fn remove_image(&mut self, id: ResourceId) {
        if let Some(elems) = self.images.get_mut(id.as_idx()) {
            elems.iter_mut().for_each(|e| e.clear());
        }
    }

    fn get_image_entries(&mut self, region: &ImageRegion) -> &mut [InternalGlobalImageUsage] {
        let idx = region.id.as_idx();
        let array_elem = region.array_elem as usize;
        let base_mip = region.base_mip_level as usize;
        let mip_count = region.mip_count as usize;
        let total_mips = base_mip + mip_count;

        if self.images.len() <= idx {
            self.images.resize(idx + 1, Vec::default());
        }

        let array_elems = &mut self.images[idx];

        if array_elems.len() <= array_elem {
            array_elems.resize(array_elem + 1, Vec::with_capacity(total_mips));
        }

        let mips = &mut array_elems[array_elem];

        if mips.len() < total_mips {
            mips.resize(total_mips, InternalGlobalImageUsage::default());
        }

        &mut mips[base_mip..total_mips]
    }

    fn get_buffer_entry(&mut self, region: &BufferRegion) -> &mut InternalGlobalBufferUsage {
        let idx = region.id.as_idx();
        let array_elem = region.array_elem as usize;

        if self.buffers.len() <= idx {
            self.buffers.resize(idx + 1, Vec::default());
        }

        let array_elems = &mut self.buffers[idx];

        if array_elems.len() <= array_elem {
            array_elems.resize(array_elem + 1, InternalGlobalBufferUsage::default());
        }

        &mut array_elems[array_elem]
    }

    fn get_set_entry(&mut self, id: ResourceId) -> &mut GlobalSetUsage {
        let idx = id.as_idx();

        if self.sets.len() <= idx {
            self.sets.resize(idx + 1, GlobalSetUsage::default());
        }

        &mut self.sets[idx]
    }
}

impl SubResourceUsage {
    #[inline(always)]
    pub fn is_write(&self) -> bool {
        const READ_ACCESSES: vk::AccessFlags2 = vk::AccessFlags2::from_raw(
            vk::AccessFlags2::INDIRECT_COMMAND_READ.as_raw()
                | vk::AccessFlags2::INDEX_READ.as_raw()
                | vk::AccessFlags2::VERTEX_ATTRIBUTE_READ.as_raw()
                | vk::AccessFlags2::UNIFORM_READ.as_raw()
                | vk::AccessFlags2::INPUT_ATTACHMENT_READ.as_raw()
                | vk::AccessFlags2::SHADER_READ.as_raw()
                | vk::AccessFlags2::COLOR_ATTACHMENT_READ.as_raw()
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ.as_raw()
                | vk::AccessFlags2::TRANSFER_READ.as_raw()
                | vk::AccessFlags2::HOST_READ.as_raw()
                | vk::AccessFlags2::MEMORY_READ.as_raw()
                | vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw()
                | vk::AccessFlags2::SHADER_STORAGE_READ.as_raw(),
        );

        READ_ACCESSES | self.access != READ_ACCESSES
    }

    #[inline(always)]
    pub fn requires_barrier(&self, other: &Self) -> bool {
        self.is_write() || other.is_write()
    }
}

impl GlobalBufferUsage {
    #[inline(always)]
    pub fn into_barrier(&self, other: &Self, sharing_mode: SharingMode) -> Option<PipelineBarrier> {
        if requires_ownership_transfer(self.queue.as_ref(), other.queue.as_ref()) {
            match sharing_mode {
                SharingMode::Exclusive => Some(PipelineBarrier::Buffer(
                    vk::BufferMemoryBarrier2::builder()
                        .src_access_mask(vk::AccessFlags2::empty())
                        .src_stage_mask(vk::PipelineStageFlags2::empty())
                        .dst_access_mask(other.sub_resource.access)
                        .dst_stage_mask(other.sub_resource.stage)
                        .build(),
                )),
                SharingMode::Concurrent => Some(PipelineBarrier::Memory(
                    vk::MemoryBarrier2::builder()
                        .src_access_mask(vk::AccessFlags2::empty())
                        .src_stage_mask(vk::PipelineStageFlags2::empty())
                        .dst_access_mask(other.sub_resource.access)
                        .dst_stage_mask(other.sub_resource.stage)
                        .build(),
                )),
            }
        } else if self.sub_resource.requires_barrier(&other.sub_resource) {
            Some(PipelineBarrier::Memory(
                vk::MemoryBarrier2::builder()
                    .src_access_mask(self.sub_resource.access)
                    .src_stage_mask(self.sub_resource.stage)
                    .dst_access_mask(other.sub_resource.access)
                    .dst_stage_mask(other.sub_resource.stage)
                    .build(),
            ))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn same_command(&self, other: &Self) -> bool {
        match (&self.queue, &other.queue) {
            (Some(us), Some(them)) => us == them,
            _ => false,
        }
    }
}

impl Default for GlobalImageUsage {
    fn default() -> Self {
        GlobalImageUsage {
            queue: None,
            sub_resource: SubResourceUsage {
                access: vk::AccessFlags2::empty(),
                stage: vk::PipelineStageFlags2::empty(),
            },
            layout: vk::ImageLayout::UNDEFINED,
        }
    }
}

impl GlobalImageUsage {
    #[inline(always)]
    pub fn into_barrier(&self, other: &Self, sharing_mode: SharingMode) -> Option<PipelineBarrier> {
        let req_owner_transfer =
            requires_ownership_transfer(self.queue.as_ref(), other.queue.as_ref())
                && sharing_mode != SharingMode::Concurrent;

        if req_owner_transfer
            || (other.layout != vk::ImageLayout::UNDEFINED && self.layout != other.layout)
        {
            Some(PipelineBarrier::Image(
                vk::ImageMemoryBarrier2::builder()
                    .src_access_mask(if req_owner_transfer {
                        vk::AccessFlags2::empty()
                    } else {
                        self.sub_resource.access
                    })
                    .src_stage_mask(if req_owner_transfer {
                        vk::PipelineStageFlags2::empty()
                    } else {
                        self.sub_resource.stage
                    })
                    .dst_access_mask(other.sub_resource.access)
                    .dst_stage_mask(other.sub_resource.stage)
                    .old_layout(self.layout)
                    .new_layout(other.layout)
                    .build(),
            ))
        } else if self.sub_resource.requires_barrier(&other.sub_resource) {
            Some(PipelineBarrier::Memory(
                vk::MemoryBarrier2::builder()
                    .src_access_mask(self.sub_resource.access)
                    .src_stage_mask(self.sub_resource.stage)
                    .dst_access_mask(other.sub_resource.access)
                    .dst_stage_mask(other.sub_resource.stage)
                    .build(),
            ))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn same_command(&self, other: &Self) -> bool {
        match (&self.queue, &other.queue) {
            (Some(us), Some(them)) => us == them,
            _ => false,
        }
    }
}

#[inline(always)]
fn requires_ownership_transfer(old: Option<&QueueUsage>, new: Option<&QueueUsage>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => old.queue != new.queue,
        _ => false,
    }
}

impl From<QueueUsage> for InternalQueueUsage {
    fn from(value: QueueUsage) -> Self {
        InternalQueueUsage {
            queue: value.queue,
            timeline_value: value.timeline_value,
            is_async: value.is_async,
        }
    }
}
