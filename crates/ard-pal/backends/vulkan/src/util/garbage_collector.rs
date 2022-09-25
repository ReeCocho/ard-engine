use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use ash::vk;
use crossbeam_channel::{Receiver, Sender};
use gpu_allocator::vulkan::{Allocation, Allocator};

use crate::{
    buffer::BufferRefCounter,
    descriptor_set::{Binding, BoundValue},
    texture::TextureRefCounter,
};

use super::{
    descriptor_pool::DescriptorPools, fast_int_hasher::FIHashMap, pipeline_cache::PipelineCache,
};

pub(crate) struct GarbageCollector {
    sender: Sender<Garbage>,
    receiver: Receiver<Garbage>,
    garbage_id: AtomicU32,
    to_destroy: Mutex<FIHashMap<u32, ToDestroy>>,
    marked: Mutex<Vec<u32>>,
}

pub(crate) enum Garbage {
    PipelineLayout(vk::PipelineLayout),
    Pipeline(vk::Pipeline),
    Buffer {
        buffer: vk::Buffer,
        allocation: Allocation,
        ref_counter: BufferRefCounter,
    },
    Texture {
        image: vk::Image,
        views: Vec<vk::ImageView>,
        allocation: Allocation,
        ref_counter: TextureRefCounter,
    },
    DescriptorSet {
        set: vk::DescriptorSet,
        layout: vk::DescriptorSetLayout,
        bindings: Vec<Vec<Option<Binding>>>,
    },
}

#[derive(Copy, Clone)]
pub(crate) struct TimelineValues {
    pub main: u64,
    pub transfer: u64,
    pub compute: u64,
}

struct ToDestroy {
    garbage: Garbage,
    values: TimelineValues,
}

impl GarbageCollector {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            sender,
            receiver,
            to_destroy: Mutex::new(HashMap::default()),
            garbage_id: AtomicU32::new(0),
            marked: Mutex::new(Vec::default()),
        }
    }

    pub fn sender(&self) -> Sender<Garbage> {
        self.sender.clone()
    }

    pub unsafe fn cleanup_all(
        &self,
        device: &ash::Device,
        allocator: &mut Allocator,
        pools: &mut DescriptorPools,
        pipelines: &mut PipelineCache,
        current: TimelineValues,
        target: TimelineValues,
    ) {
        loop {
            self.cleanup(device, allocator, pools, pipelines, current, target);
            if self.to_destroy.lock().unwrap().is_empty() {
                break;
            }
        }
    }

    pub unsafe fn cleanup(
        &self,
        device: &ash::Device,
        allocator: &mut Allocator,
        pools: &mut DescriptorPools,
        pipelines: &mut PipelineCache,
        current: TimelineValues,
        target: TimelineValues,
    ) {
        // Receive all incoming garbage
        let mut to_destroy = self.to_destroy.lock().unwrap();
        while let Ok(garbage) = self.receiver.try_recv() {
            let id = self.garbage_id.fetch_add(1, Ordering::Relaxed);
            to_destroy.insert(
                id,
                ToDestroy {
                    garbage,
                    values: target,
                },
            );
        }

        // Mark everything that is not being used by any queue
        let mut marked = self.marked.lock().unwrap();
        marked.clear();
        for (id, garbage) in to_destroy.iter() {
            match &garbage.garbage {
                Garbage::Buffer { ref_counter, .. } => {
                    if !ref_counter.is_last() {
                        continue;
                    }
                }
                Garbage::Texture { ref_counter, .. } => {
                    if !ref_counter.is_last() {
                        continue;
                    }
                }
                _ => {}
            }

            if garbage.values.main <= current.main
                && garbage.values.transfer <= current.transfer
                && garbage.values.compute <= current.compute
            {
                marked.push(*id);
            }
        }

        // Remove marked elements from the list
        for id in marked.iter().rev() {
            match to_destroy.remove(id).unwrap().garbage {
                Garbage::PipelineLayout(layout) => {
                    // Also destroy associated pipelines
                    pipelines.release(device, layout);
                    device.destroy_pipeline_layout(layout, None);
                }
                Garbage::Pipeline(pipeline) => {
                    device.destroy_pipeline(pipeline, None);
                }
                Garbage::Buffer {
                    buffer, allocation, ..
                } => {
                    device.destroy_buffer(buffer, None);
                    allocator.free(allocation).unwrap();
                }
                Garbage::Texture {
                    image,
                    views,
                    allocation,
                    ..
                } => {
                    device.destroy_image(image, None);
                    for view in views {
                        device.destroy_image_view(view, None);
                    }
                    allocator.free(allocation).unwrap();
                }
                Garbage::DescriptorSet {
                    set,
                    layout,
                    bindings,
                } => {
                    pools.get_by_layout(layout).unwrap().free(set);
                    for binding in bindings {
                        for element in binding {
                            if let Some(element) = element {
                                match element.value {
                                    BoundValue::Texture { view, .. } => {
                                        device.destroy_image_view(view, None);
                                    }
                                    BoundValue::StorageImage { view, .. } => {
                                        device.destroy_image_view(view, None);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
