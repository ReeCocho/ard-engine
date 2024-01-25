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
    descriptor_set::{BoundValue, DescriptorSetBindings},
    texture::TextureRefCounter,
};

use super::{
    descriptor_pool::DescriptorPools,
    fast_int_hasher::FIHashMap,
    pipeline_cache::PipelineCache,
    usage::{BufferRegion, GlobalResourceUsage, ImageRegion},
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
        array_elements: usize,
        allocation: Allocation,
        ref_counter: BufferRefCounter,
    },
    Texture {
        image: vk::Image,
        array_elements: usize,
        mips: usize,
        views: Vec<vk::ImageView>,
        allocation: Allocation,
        ref_counter: TextureRefCounter,
    },
    DescriptorSet {
        set: vk::DescriptorSet,
        layout: vk::DescriptorSetLayout,
        bindings: DescriptorSetBindings,
    },
}

#[derive(Copy, Clone)]
pub(crate) struct TimelineValues {
    pub main: u64,
    pub transfer: u64,
    pub compute: u64,
}

pub(crate) struct GarbageCleanupArgs<'a> {
    pub device: &'a ash::Device,
    pub allocator: &'a mut Allocator,
    pub pools: &'a mut DescriptorPools,
    pub pipelines: &'a mut PipelineCache,
    pub global_usage: &'a mut GlobalResourceUsage,
    pub current: TimelineValues,
    pub target: TimelineValues,
    pub override_ref_counter: bool,
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

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.to_destroy.lock().unwrap().is_empty()
    }

    pub fn sender(&self) -> Sender<Garbage> {
        self.sender.clone()
    }

    pub unsafe fn cleanup(&self, args: GarbageCleanupArgs) {
        // Receive all incoming garbage
        let mut to_destroy = self.to_destroy.lock().unwrap();
        while let Ok(garbage) = self.receiver.try_recv() {
            let id = self.garbage_id.fetch_add(1, Ordering::Relaxed);
            to_destroy.insert(
                id,
                ToDestroy {
                    garbage,
                    values: args.target,
                },
            );
        }

        // Mark everything that is not being used by any queue
        let mut marked = self.marked.lock().unwrap();
        marked.clear();
        for (id, garbage) in to_destroy.iter() {
            if !args.override_ref_counter {
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
            }

            if garbage.values.main <= args.current.main
                && garbage.values.transfer <= args.current.transfer
                && garbage.values.compute <= args.current.compute
            {
                marked.push(*id);
            }
        }

        // Remove marked elements from the list
        for id in marked.iter().rev() {
            match to_destroy.remove(id).unwrap().garbage {
                Garbage::PipelineLayout(layout) => {
                    // Also destroy associated pipelines
                    args.pipelines.release(args.device, layout);
                    args.device.destroy_pipeline_layout(layout, None);
                }
                Garbage::Pipeline(pipeline) => {
                    args.device.destroy_pipeline(pipeline, None);
                }
                Garbage::Buffer {
                    buffer,
                    allocation,
                    array_elements,
                    ..
                } => {
                    args.device.destroy_buffer(buffer, None);
                    args.allocator.free(allocation).unwrap();

                    for i in 0..array_elements {
                        args.global_usage.remove_buffer(&BufferRegion {
                            buffer,
                            array_elem: i as u32,
                        });
                    }
                }
                Garbage::Texture {
                    image,
                    views,
                    allocation,
                    array_elements,
                    mips,
                    ..
                } => {
                    args.device.destroy_image(image, None);
                    for view in views {
                        args.device.destroy_image_view(view, None);
                    }
                    args.allocator.free(allocation).unwrap();

                    for a in 0..array_elements {
                        for m in 0..mips {
                            args.global_usage.remove_image(&ImageRegion {
                                image,
                                array_elem: a as u32,
                                mip_level: m as u32,
                            });
                        }
                    }
                }
                Garbage::DescriptorSet {
                    set,
                    layout,
                    bindings,
                } => {
                    args.pools.get_by_layout(layout).unwrap().free(set);
                    for element in bindings.into_iter().flatten().flatten() {
                        match element.value {
                            BoundValue::Texture { view, .. } => {
                                args.device.destroy_image_view(view, None);
                            }
                            BoundValue::CubeMap { view, .. } => {
                                args.device.destroy_image_view(view, None);
                            }
                            BoundValue::StorageImage { view, .. } => {
                                args.device.destroy_image_view(view, None);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
