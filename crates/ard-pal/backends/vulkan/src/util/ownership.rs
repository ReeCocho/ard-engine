use api::types::{CubeFace, QueueType};
use ash::vk;
use rustc_hash::FxHashMap;

use crate::{buffer::Buffer, cube_map::CubeMap, texture::Texture, QueueFamilyIndices};

use super::usage::{BufferRegion, GlobalResourceUsage, ImageRegion};

pub struct OwnershipTransferTracker<'a> {
    src_queue: u32,
    qfi: &'a QueueFamilyIndices,
    /// Buffers to transfer ownership of.
    buffers: FxHashMap<BufferRegion, vk::BufferMemoryBarrier>,
    /// Images to transfer ownership of.
    images: FxHashMap<ImageRegion, vk::ImageMemoryBarrier>,
}

impl<'a> OwnershipTransferTracker<'a> {
    pub(crate) fn new(src_queue: QueueType, qfi: &'a QueueFamilyIndices) -> Self {
        Self {
            src_queue: qfi.to_index(src_queue),
            qfi,
            buffers: FxHashMap::default(),
            images: FxHashMap::default(),
        }
    }

    #[inline(always)]
    pub fn register_buffer(&mut self, buffer: &Buffer, array_elem: usize, new_queue: QueueType) {
        self.buffers.insert(
            BufferRegion {
                buffer: buffer.buffer,
                array_elem: array_elem as u32,
            },
            vk::BufferMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::NONE)
                .src_queue_family_index(self.src_queue)
                .dst_queue_family_index(self.qfi.to_index(new_queue))
                .buffer(buffer.buffer)
                .offset(buffer.offset(array_elem))
                .size(buffer.aligned_size)
                .build(),
        );
    }

    #[inline(always)]
    pub fn register_texture(
        &mut self,
        texture: &Texture,
        array_elem: usize,
        base_mip: u32,
        mip_count: u32,
        new_queue: QueueType,
    ) {
        for mip_level in base_mip..(base_mip + mip_count) {
            self.images.insert(
                ImageRegion {
                    image: texture.image,
                    array_elem: array_elem as u32,
                    mip_level,
                },
                vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::NONE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::UNDEFINED)
                    .src_queue_family_index(self.src_queue)
                    .dst_queue_family_index(self.qfi.to_index(new_queue))
                    .image(texture.image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: texture.aspect_flags,
                        base_mip_level: mip_level,
                        level_count: 1,
                        base_array_layer: array_elem as u32,
                        layer_count: 1,
                    })
                    .build(),
            );
        }
    }

    #[inline(always)]
    pub fn register_cube_map(
        &mut self,
        cube_map: &CubeMap,
        array_elem: usize,
        base_mip: u32,
        mip_count: u32,
        face: CubeFace,
        new_queue: QueueType,
    ) {
        let array_elem = CubeMap::to_array_elem(array_elem, face);

        for mip_level in base_mip..(base_mip + mip_count) {
            self.images.insert(
                ImageRegion {
                    image: cube_map.image,
                    array_elem: array_elem as u32,
                    mip_level,
                },
                vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::NONE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::UNDEFINED)
                    .src_queue_family_index(self.src_queue)
                    .dst_queue_family_index(self.qfi.to_index(new_queue))
                    .image(cube_map.image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: cube_map.aspect_flags,
                        base_mip_level: mip_level,
                        level_count: 1,
                        base_array_layer: array_elem as u32,
                        layer_count: 1,
                    })
                    .build(),
            );
        }
    }

    pub(crate) unsafe fn transfer_ownership(
        mut self,
        global: &GlobalResourceUsage,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
    ) {
        // Update images with correct layouts
        self.images.iter_mut().for_each(|(region, barrier)| {
            barrier.old_layout = global.get_layout(region);
            barrier.new_layout = global.get_layout(region);
        });

        // Convert into vecs
        let buffer_barriers = self.buffers.into_values().collect::<Vec<_>>();
        let image_barriers = self.images.into_values().collect::<Vec<_>>();

        // Run the barriers
        if !buffer_barriers.is_empty() || !image_barriers.is_empty() {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::DependencyFlags::BY_REGION,
                &[],
                &buffer_barriers,
                &image_barriers,
            );
        }
    }
}
