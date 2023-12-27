use api::{
    descriptor_set::{
        DescriptorBinding, DescriptorSetCreateError, DescriptorSetCreateInfo,
        DescriptorSetLayoutCreateError, DescriptorSetLayoutCreateInfo, DescriptorSetUpdate,
        DescriptorType, DescriptorValue,
    },
    types::{AccessType, QueueTypes, ShaderStage, SharingMode},
    Backend,
};
use ash::vk;
use crossbeam_channel::Sender;

use crate::{
    buffer::BufferRefCounter,
    job::Job,
    texture::TextureRefCounter,
    util::{descriptor_pool::DescriptorPools, garbage_collector::Garbage},
    VulkanBackend,
};

pub struct DescriptorSet {
    pub(crate) set: vk::DescriptorSet,
    pub(crate) layout: vk::DescriptorSetLayout,
    pub(crate) bound: Vec<Vec<Option<Binding>>>,
    pub(crate) on_drop: Sender<Garbage>,
}

pub struct DescriptorSetLayout {
    pub(crate) descriptor: DescriptorSetLayoutCreateInfo,
    pub(crate) layout: vk::DescriptorSetLayout,
}

pub(crate) struct Binding {
    pub value: BoundValue,
    pub stage: vk::PipelineStageFlags,
    pub access: vk::AccessFlags,
}

pub(crate) enum BoundValue {
    UniformBuffer {
        _ref_counter: BufferRefCounter,
        buffer: vk::Buffer,
        queue_types: QueueTypes,
        sharing_mode: SharingMode,
        array_element: usize,
        aligned_size: usize,
    },
    StorageBuffer {
        _ref_counter: BufferRefCounter,
        buffer: vk::Buffer,
        queue_types: QueueTypes,
        sharing_mode: SharingMode,
        array_element: usize,
        aligned_size: usize,
    },
    StorageImage {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        view: vk::ImageView,
        queue_types: QueueTypes,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        mip: u32,
        array_element: usize,
    },
    Texture {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        view: vk::ImageView,
        queue_types: QueueTypes,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        base_mip: u32,
        mip_count: u32,
        array_element: usize,
    },
    CubeMap {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        view: vk::ImageView,
        queue_types: QueueTypes,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        base_mip: u32,
        mip_count: u32,
        array_element: usize,
    },
}

impl DescriptorSetLayout {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        pools: &mut DescriptorPools,
        create_info: DescriptorSetLayoutCreateInfo,
    ) -> Result<Self, DescriptorSetLayoutCreateError> {
        // Pre-cache the pool
        let pool = pools.get(device, create_info.clone());
        Ok(DescriptorSetLayout {
            descriptor: create_info,
            layout: pool.layout(),
        })
    }

    #[inline]
    pub(crate) fn get_binding(&self, binding_value: u32) -> Option<&DescriptorBinding> {
        self.descriptor
            .bindings
            .iter()
            .find(|&binding| binding.binding == binding_value)
    }
}

impl DescriptorSet {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        pools: &mut DescriptorPools,
        garbage: Sender<Garbage>,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        create_info: DescriptorSetCreateInfo<crate::VulkanBackend>,
    ) -> Result<Self, DescriptorSetCreateError> {
        let mut bound = Vec::with_capacity(create_info.layout.internal().descriptor.bindings.len());
        for binding in &create_info.layout.internal().descriptor.bindings {
            let mut binds = Vec::with_capacity(binding.count);
            binds.resize_with(binding.count, || None);
            bound.push(binds);
        }

        let pool = pools.get(device, create_info.layout.internal().descriptor.clone());
        let set = pool.allocate(device, debug, create_info.debug_name);
        Ok(DescriptorSet {
            set,
            layout: pool.layout(),
            on_drop: garbage,
            bound,
        })
    }

    pub(crate) unsafe fn update(
        &mut self,
        ctx: &VulkanBackend,
        layout: &DescriptorSetLayout,
        updates: &[DescriptorSetUpdate<crate::VulkanBackend>],
    ) {
        let sampleable_aspects = vk::ImageAspectFlags::COLOR | vk::ImageAspectFlags::DEPTH;

        // Wait until the last queue that the buffer was used in has finished it's work
        let mut resc_state = ctx.resource_state.write().unwrap();
        let mut sampler_cache = ctx.samplers.lock().unwrap();

        // NOTE: The reason we set the usage to `None` is because we have to wait for the previous
        // usage to complete. This implies that no one is using this set anymore and thus no
        // waits are further needed.
        if let Some(old) = resc_state.register_set(self.set, None) {
            ctx.wait_on(
                &Job {
                    ty: old.queue,
                    target_value: old.timeline_value,
                },
                None,
            );
        }

        let mut writes = Vec::with_capacity(updates.len());
        let mut buffers = Vec::with_capacity(updates.len());
        let mut images = Vec::with_capacity(updates.len());

        for update in updates {
            // Deal with the old value
            if let Some(old) = self.bound[update.binding as usize][update.array_element].take() {
                match old.value {
                    // It's safe to destroy the image view now because we guarantee the set is not
                    // being used by
                    BoundValue::Texture { view, .. } => {
                        ctx.device.destroy_image_view(view, None);
                    }
                    BoundValue::CubeMap { view, .. } => {
                        ctx.device.destroy_image_view(view, None);
                    }
                    BoundValue::StorageImage { view, .. } => {
                        ctx.device.destroy_image_view(view, None);
                    }
                    _ => {}
                }
            }

            // Bind new value
            self.bound[update.binding as usize][update.array_element] = Some({
                let binding = layout.get_binding(update.binding).unwrap();
                let access = match binding.ty {
                    DescriptorType::Texture => vk::AccessFlags::SHADER_READ,
                    DescriptorType::UniformBuffer => vk::AccessFlags::UNIFORM_READ,
                    DescriptorType::StorageBuffer(ty) => match ty {
                        AccessType::Read => vk::AccessFlags::SHADER_READ,
                        AccessType::ReadWrite => {
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE
                        }
                    },
                    DescriptorType::StorageImage(ty) => match ty {
                        AccessType::Read => vk::AccessFlags::SHADER_READ,
                        AccessType::ReadWrite => {
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE
                        }
                    },
                    DescriptorType::CubeMap => vk::AccessFlags::SHADER_READ,
                };
                let stage = match binding.stage {
                    ShaderStage::Vertex => vk::PipelineStageFlags::VERTEX_SHADER,
                    ShaderStage::Fragment => vk::PipelineStageFlags::FRAGMENT_SHADER,
                    ShaderStage::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                    ShaderStage::AllStages => {
                        vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER
                            | vk::PipelineStageFlags::COMPUTE_SHADER
                    }
                    ShaderStage::AllGraphics => {
                        vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER
                    }
                };

                match &update.value {
                    DescriptorValue::UniformBuffer {
                        buffer,
                        array_element,
                    } => {
                        let queue_types = buffer.queue_types();
                        let sharing_mode = buffer.sharing_mode();
                        let buffer = buffer.internal();

                        buffers.push(
                            vk::DescriptorBufferInfo::builder()
                                .buffer(buffer.buffer)
                                .offset(buffer.aligned_size * (*array_element) as u64)
                                .range(buffer.aligned_size)
                                .build(),
                        );

                        writes.push(
                            vk::WriteDescriptorSet::builder()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                                .buffer_info(&buffers[buffers.len() - 1..])
                                .build(),
                        );

                        Binding {
                            access,
                            stage,
                            value: BoundValue::UniformBuffer {
                                _ref_counter: buffer.ref_counter.clone(),
                                buffer: buffer.buffer,
                                queue_types,
                                sharing_mode,
                                array_element: *array_element,
                                aligned_size: buffer.aligned_size as usize,
                            },
                        }
                    }
                    DescriptorValue::StorageBuffer {
                        buffer,
                        array_element,
                    } => {
                        let queue_types = buffer.queue_types();
                        let sharing_mode = buffer.sharing_mode();
                        let buffer = buffer.internal();

                        buffers.push(
                            vk::DescriptorBufferInfo::builder()
                                .buffer(buffer.buffer)
                                .offset(buffer.offset(*array_element))
                                .range(buffer.aligned_size)
                                .build(),
                        );

                        writes.push(
                            vk::WriteDescriptorSet::builder()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&buffers[buffers.len() - 1..])
                                .build(),
                        );

                        Binding {
                            access,
                            stage,
                            value: BoundValue::StorageBuffer {
                                _ref_counter: buffer.ref_counter.clone(),
                                buffer: buffer.buffer,
                                queue_types,
                                sharing_mode,
                                array_element: *array_element,
                                aligned_size: buffer.aligned_size as usize,
                            },
                        }
                    }
                    DescriptorValue::Texture {
                        texture,
                        array_element,
                        sampler,
                        base_mip,
                        mip_count,
                    } => {
                        let queue_types = texture.queue_types();
                        let sharing_mode = texture.sharing_mode();
                        let texture = texture.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::builder()
                            .format(texture.format)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: texture.aspect_flags & sampleable_aspects,
                                base_mip_level: *base_mip as u32,
                                level_count: *mip_count as u32,
                                base_array_layer: *array_element as u32,
                                layer_count: 1,
                            })
                            .components(vk::ComponentMapping {
                                r: vk::ComponentSwizzle::R,
                                g: vk::ComponentSwizzle::G,
                                b: vk::ComponentSwizzle::B,
                                a: vk::ComponentSwizzle::A,
                            })
                            .image(texture.image)
                            .build();

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::builder()
                                .sampler(sampler_cache.get(&ctx.device, *sampler))
                                .image_view(view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .build(),
                        );

                        writes.push(
                            vk::WriteDescriptorSet::builder()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                                .image_info(&images[images.len() - 1..])
                                .build(),
                        );

                        Binding {
                            access,
                            stage,
                            value: BoundValue::Texture {
                                _ref_counter: texture.ref_counter.clone(),
                                image: texture.image,
                                queue_types,
                                sharing_mode,
                                view,
                                aspect_mask: texture.aspect_flags,
                                mip_count: *mip_count as u32,
                                base_mip: *base_mip as u32,
                                array_element: *array_element,
                            },
                        }
                    }
                    DescriptorValue::StorageImage {
                        texture,
                        array_element,
                        mip,
                    } => {
                        let queue_types = texture.queue_types();
                        let sharing_mode = texture.sharing_mode();
                        let texture = texture.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::builder()
                            .format(texture.format)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: texture.aspect_flags & sampleable_aspects,
                                base_mip_level: *mip as u32,
                                level_count: 1,
                                base_array_layer: *array_element as u32,
                                layer_count: 1,
                            })
                            .components(vk::ComponentMapping {
                                r: vk::ComponentSwizzle::R,
                                g: vk::ComponentSwizzle::G,
                                b: vk::ComponentSwizzle::B,
                                a: vk::ComponentSwizzle::A,
                            })
                            .image(texture.image)
                            .build();

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::builder()
                                .image_view(view)
                                .image_layout(vk::ImageLayout::GENERAL)
                                .build(),
                        );

                        writes.push(
                            vk::WriteDescriptorSet::builder()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                                .image_info(&images[images.len() - 1..])
                                .build(),
                        );

                        Binding {
                            access,
                            stage,
                            value: BoundValue::StorageImage {
                                _ref_counter: texture.ref_counter.clone(),
                                image: texture.image,
                                view,
                                queue_types,
                                sharing_mode,
                                aspect_mask: texture.aspect_flags,
                                mip: *mip as u32,
                                array_element: *array_element,
                            },
                        }
                    }
                    DescriptorValue::CubeMap {
                        cube_map,
                        array_element,
                        sampler,
                        base_mip,
                        mip_count,
                    } => {
                        let queue_types = cube_map.queue_types();
                        let sharing_mode = cube_map.sharing_mode();
                        let cube_map = cube_map.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::builder()
                            .format(cube_map.format)
                            .view_type(vk::ImageViewType::CUBE)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: cube_map.aspect_flags & sampleable_aspects,
                                base_mip_level: *base_mip as u32,
                                level_count: *mip_count as u32,
                                base_array_layer: 6 * *array_element as u32,
                                layer_count: 6,
                            })
                            .components(vk::ComponentMapping {
                                r: vk::ComponentSwizzle::R,
                                g: vk::ComponentSwizzle::G,
                                b: vk::ComponentSwizzle::B,
                                a: vk::ComponentSwizzle::A,
                            })
                            .image(cube_map.image)
                            .build();

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::builder()
                                .sampler(sampler_cache.get(&ctx.device, *sampler))
                                .image_view(view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .build(),
                        );

                        writes.push(
                            vk::WriteDescriptorSet::builder()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                                .image_info(&images[images.len() - 1..])
                                .build(),
                        );

                        Binding {
                            access,
                            stage,
                            value: BoundValue::CubeMap {
                                _ref_counter: cube_map.ref_counter.clone(),
                                image: cube_map.image,
                                view,
                                queue_types,
                                sharing_mode,
                                aspect_mask: cube_map.aspect_flags,
                                mip_count: *mip_count as u32,
                                base_mip: *base_mip as u32,
                                array_element: *array_element,
                            },
                        }
                    }
                }
            });
        }

        ctx.device.update_descriptor_sets(&writes, &[]);
    }
}

impl Drop for DescriptorSet {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::DescriptorSet {
            set: self.set,
            layout: self.layout,
            bindings: std::mem::take(&mut self.bound),
        });
    }
}
