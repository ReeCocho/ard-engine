use std::{collections::VecDeque, ffi::CString};

use api::types::QueueType;
use ash::vk::{self, Handle};

use crate::util::semaphores::{SemaphoreTracker, WaitInfo};

pub(crate) struct VkQueue {
    pub queue: vk::Queue,
    ty: QueueType,
    /// All commands submitted to this queue must be allocated from this pool.
    command_pool: vk::CommandPool,
    /// Queue of free command buffers.
    free: VecDeque<ActiveCommandBuffer>,
    /// Total number of command buffers.
    command_buffer_count: usize,
    /// All work performed on this queue increments the value of this semaphore.
    semaphore: vk::Semaphore,
    /// The timeline semaphore value this queue will set when work is complete.
    target_value: u64,
}

struct ActiveCommandBuffer {
    pub command_buffer: vk::CommandBuffer,
    /// What value the timeline semaphore must have for this command buffers work to be complete.
    pub target: u64,
}

impl VkQueue {
    pub unsafe fn new(
        device: &ash::Device,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        queue: vk::Queue,
        ty: QueueType,
        queue_family: u32,
    ) -> Result<Self, vk::Result> {
        // Create timeline semaphore
        let mut type_create_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0)
            .build();
        let create_info = vk::SemaphoreCreateInfo::builder()
            .push_next(&mut type_create_info)
            .flags(vk::SemaphoreCreateFlags::empty())
            .build();
        let semaphore = device.create_semaphore(&create_info, None)?;

        // Create command pool
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family)
            .build();
        let command_pool = device.create_command_pool(&create_info, None)?;

        // Name objects
        if let Some(debug) = debug {
            let (queue_name, semaphore_name, pool_name) = match ty {
                QueueType::Main => (
                    CString::new("main_queue").unwrap(),
                    CString::new("main_semaphore").unwrap(),
                    CString::new("main_pool").unwrap(),
                ),
                QueueType::Transfer => (
                    CString::new("transfer_queue").unwrap(),
                    CString::new("transfer_semaphore").unwrap(),
                    CString::new("transfer_pool").unwrap(),
                ),
                QueueType::Compute => (
                    CString::new("compute_queue").unwrap(),
                    CString::new("compute_semaphore").unwrap(),
                    CString::new("compute_pool").unwrap(),
                ),
                QueueType::Present => (
                    CString::new("present_queue").unwrap(),
                    CString::new("present_semaphore").unwrap(),
                    CString::new("present_pool").unwrap(),
                ),
            };

            let queue_name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(vk::ObjectType::QUEUE)
                .object_handle(queue.as_raw())
                .object_name(&queue_name)
                .build();

            let semaphore_name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(vk::ObjectType::SEMAPHORE)
                .object_handle(semaphore.as_raw())
                .object_name(&semaphore_name)
                .build();

            let pool_name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(vk::ObjectType::COMMAND_POOL)
                .object_handle(command_pool.as_raw())
                .object_name(&pool_name)
                .build();

            debug
                .debug_utils_set_object_name(device.handle(), &queue_name_info)
                .unwrap();
            debug
                .debug_utils_set_object_name(device.handle(), &semaphore_name_info)
                .unwrap();
            debug
                .debug_utils_set_object_name(device.handle(), &pool_name_info)
                .unwrap();
        }

        Ok(Self {
            queue,
            semaphore,
            ty,
            command_pool,
            free: VecDeque::default(),
            command_buffer_count: 0,
            target_value: 0,
        })
    }

    #[inline(always)]
    pub fn semaphore(&self) -> vk::Semaphore {
        self.semaphore
    }

    #[inline(always)]
    pub fn target_timeline_value(&self) -> u64 {
        self.target_value
    }

    #[inline(always)]
    pub unsafe fn current_timeline_value(&self, device: &ash::Device) -> u64 {
        device.get_semaphore_counter_value(self.semaphore).unwrap()
    }

    pub unsafe fn allocate_command_buffer(
        &mut self,
        device: &ash::Device,
        debug: Option<&ash::extensions::ext::DebugUtils>,
    ) -> vk::CommandBuffer {
        // Check current timeline value
        let cur_value = device.get_semaphore_counter_value(self.semaphore).unwrap();

        // Attempt to get free command buffer
        let command_buffer = if let Some(free) = self.free.front() {
            if cur_value >= free.target {
                self.free.pop_front()
            } else {
                None
            }
        } else {
            None
        };

        match command_buffer {
            Some(cb) => cb.command_buffer,
            // If there was no free command buffer, we will allocate one
            None => {
                let alloc_info = vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(self.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build();
                let cb = device.allocate_command_buffers(&alloc_info).unwrap()[0];

                // Name the command buffer
                if let Some(debug) = debug {
                    let name = match self.ty {
                        QueueType::Main => CString::new(format!(
                            "main_command_buffer_{}",
                            self.command_buffer_count
                        )),
                        QueueType::Transfer => CString::new(format!(
                            "transfer_command_buffer_{}",
                            self.command_buffer_count
                        )),
                        QueueType::Compute => CString::new(format!(
                            "compute_command_buffer_{}",
                            self.command_buffer_count
                        )),
                        QueueType::Present => CString::new(format!(
                            "present_command_buffer_{}",
                            self.command_buffer_count
                        )),
                    }
                    .unwrap();

                    let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                        .object_type(vk::ObjectType::COMMAND_BUFFER)
                        .object_handle(cb.as_raw())
                        .object_name(&name)
                        .build();
                    debug
                        .debug_utils_set_object_name(device.handle(), &name_info)
                        .unwrap();
                }

                self.command_buffer_count += 1;
                cb
            }
        }
    }

    pub unsafe fn submit(
        &mut self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        mut semaphore_tracker: SemaphoreTracker,
    ) -> ash::prelude::VkResult<()> {
        // Always signal and wait on ourselves
        semaphore_tracker.register_wait(
            self.semaphore,
            WaitInfo {
                value: Some(self.target_value),
                stage: vk::PipelineStageFlags::TOP_OF_PIPE,
            },
        );
        semaphore_tracker.register_signal(self.semaphore, Some(self.target_value + 1));
        self.target_value += 1;
        let semaphores = semaphore_tracker.finish();

        // Put the command buffer into our free stack
        self.free.push_back(ActiveCommandBuffer {
            command_buffer,
            target: self.target_value,
        });

        // Semaphores
        let mut signals = Vec::with_capacity(semaphores.signals.len());
        let mut signal_values = Vec::with_capacity(semaphores.signals.len());
        let mut waits = Vec::with_capacity(semaphores.waits.len());
        let mut wait_values = Vec::with_capacity(semaphores.waits.len());
        let mut wait_stages = Vec::with_capacity(semaphores.waits.len());

        // Find all semaphores
        for (semaphore, info) in &semaphores.waits {
            waits.push(*semaphore);
            wait_values.push(info.value.unwrap_or_default());
            wait_stages.push(info.stage);
        }

        for (semaphore, value) in &semaphores.signals {
            signals.push(*semaphore);
            signal_values.push(value.unwrap_or_default());
        }

        // Submit to queue
        let command_buffer = [command_buffer];
        let mut timeline_info = vk::TimelineSemaphoreSubmitInfo::builder()
            .signal_semaphore_values(&signal_values)
            .wait_semaphore_values(&wait_values)
            .build();
        let submit_info = [vk::SubmitInfo::builder()
            .command_buffers(&command_buffer)
            .signal_semaphores(&signals)
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&wait_stages)
            .push_next(&mut timeline_info)
            .build()];
        device.queue_submit(self.queue, &submit_info, vk::Fence::null())
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        device.destroy_command_pool(self.command_pool, None);
        device.destroy_semaphore(self.semaphore, None);
    }
}
