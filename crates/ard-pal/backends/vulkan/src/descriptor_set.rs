use api::{
    descriptor_set::{
        DescriptorBinding, DescriptorSetCreateError, DescriptorSetCreateInfo,
        DescriptorSetLayoutCreateError, DescriptorSetLayoutCreateInfo, DescriptorSetUpdate,
        DescriptorType, DescriptorValue,
    },
    types::{AccessType, ShaderStage, SharingMode},
    Backend,
};
use ash::vk;
use crossbeam_channel::Sender;
use smallvec::SmallVec;

use crate::{
    buffer::BufferRefCounter,
    job::Job,
    texture::TextureRefCounter,
    util::{
        descriptor_pool::DescriptorPools,
        garbage_collector::Garbage,
        id_gen::{IdGenerator, ResourceId},
    },
    VulkanBackend,
};

pub struct DescriptorSet {
    pub(crate) set: vk::DescriptorSet,
    pub(crate) id: ResourceId,
    pub(crate) layout: vk::DescriptorSetLayout,
    pub(crate) bound: DescriptorSetBindings,
    pub(crate) on_drop: Sender<Garbage>,
}

pub struct DescriptorSetLayout {
    pub(crate) descriptor: DescriptorSetLayoutCreateInfo,
    pub(crate) layout: vk::DescriptorSetLayout,
}

pub(crate) type DescriptorSetBindings = SmallVec<[SmallVec<[Option<Binding>; 1]>; 4]>;

pub(crate) struct Binding {
    pub value: BoundValue,
    pub stage: vk::PipelineStageFlags2,
    pub access: vk::AccessFlags2,
}

pub(crate) enum BoundValue {
    UniformBuffer {
        _ref_counter: BufferRefCounter,
        buffer: vk::Buffer,
        id: ResourceId,
        sharing_mode: SharingMode,
        array_element: usize,
        aligned_size: usize,
    },
    StorageBuffer {
        _ref_counter: BufferRefCounter,
        buffer: vk::Buffer,
        id: ResourceId,
        sharing_mode: SharingMode,
        array_element: usize,
        aligned_size: usize,
    },
    StorageImage {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        id: ResourceId,
        view: vk::ImageView,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        mip: u32,
        array_element: usize,
    },
    Texture {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        id: ResourceId,
        view: vk::ImageView,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        base_mip: u32,
        mip_count: u32,
        array_element: usize,
    },
    CubeMap {
        _ref_counter: TextureRefCounter,
        image: vk::Image,
        id: ResourceId,
        view: vk::ImageView,
        sharing_mode: SharingMode,
        aspect_mask: vk::ImageAspectFlags,
        base_mip: u32,
        mip_count: u32,
        array_element: usize,
    },
    Tlas {
        _ref_counter: BufferRefCounter,
        buffer: vk::Buffer,
        id: ResourceId,
        sharing_mode: SharingMode,
        aligned_size: usize,
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
        debug: Option<&ash::ext::debug_utils::Device>,
        create_info: DescriptorSetCreateInfo<crate::VulkanBackend>,
        set_ids: &IdGenerator,
    ) -> Result<Self, DescriptorSetCreateError> {
        let mut bound =
            SmallVec::with_capacity(create_info.layout.internal().descriptor.bindings.len());
        for binding in &create_info.layout.internal().descriptor.bindings {
            let mut binds = SmallVec::with_capacity(binding.count);
            binds.resize_with(binding.count, || None);
            bound.push(binds);
        }

        let pool = pools.get(device, create_info.layout.internal().descriptor.clone());
        let set = pool.allocate(device, debug, create_info.debug_name);
        Ok(DescriptorSet {
            set,
            id: set_ids.create(),
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
        const SAMPLEABLE_ASPECTS: vk::ImageAspectFlags = vk::ImageAspectFlags::from_raw(
            vk::ImageAspectFlags::COLOR.as_raw() | vk::ImageAspectFlags::DEPTH.as_raw(),
        );

        // Wait until the last queue that the buffer was used in has finished it's work
        let resc_state = ctx.resource_state.write().unwrap();
        let mut sampler_cache = ctx.samplers.lock().unwrap();

        // NOTE: The reason we set the usage to `None` is because we have to wait for the previous
        // usage to complete. This implies that no one is using this set anymore and thus no
        // waits are further needed.
        if let Some(old) = resc_state.get_set_queue_usage(self.id) {
            ctx.wait_on(
                &Job {
                    ty: old.queue,
                    target_value: old.timeline_value,
                },
                None,
            );
        }

        let mut writes = Vec::with_capacity(updates.len());
        let mut buffers = Vec::default();
        let mut images = Vec::default();
        let mut tlas_writes = Vec::default();

        for update in updates {
            // Deal with the old value
            if let Some(old) = self.bound[update.binding as usize][update.array_element].take() {
                match old.value {
                    // It's safe to destroy the image view now because we guarantee the set is not
                    // being used by any queues
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
                    DescriptorType::Texture => vk::AccessFlags2::SHADER_READ,
                    DescriptorType::UniformBuffer => vk::AccessFlags2::UNIFORM_READ,
                    DescriptorType::StorageBuffer(ty) => match ty {
                        AccessType::Read => vk::AccessFlags2::SHADER_STORAGE_READ,
                        AccessType::ReadWrite => {
                            vk::AccessFlags2::SHADER_STORAGE_READ
                                | vk::AccessFlags2::SHADER_STORAGE_WRITE
                        }
                    },
                    DescriptorType::StorageImage(ty) => match ty {
                        AccessType::Read => vk::AccessFlags2::SHADER_STORAGE_READ,
                        AccessType::ReadWrite => {
                            vk::AccessFlags2::SHADER_STORAGE_READ
                                | vk::AccessFlags2::SHADER_STORAGE_WRITE
                        }
                    },
                    DescriptorType::CubeMap => vk::AccessFlags2::SHADER_READ,
                    DescriptorType::TopLevelAccelerationStructure => {
                        vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                    }
                };
                let stage = match binding.stage {
                    ShaderStage::Vertex => vk::PipelineStageFlags2::VERTEX_SHADER,
                    ShaderStage::Fragment => vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    ShaderStage::Compute => vk::PipelineStageFlags2::COMPUTE_SHADER,
                    ShaderStage::Mesh => vk::PipelineStageFlags2::MESH_SHADER_EXT,
                    ShaderStage::Task => vk::PipelineStageFlags2::TASK_SHADER_EXT,
                    ShaderStage::AllStages => {
                        vk::PipelineStageFlags2::VERTEX_SHADER
                            | vk::PipelineStageFlags2::FRAGMENT_SHADER
                            | vk::PipelineStageFlags2::COMPUTE_SHADER
                            | vk::PipelineStageFlags2::MESH_SHADER_EXT
                            | vk::PipelineStageFlags2::TASK_SHADER_EXT
                            | vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                    }
                    ShaderStage::AllGraphics => {
                        vk::PipelineStageFlags2::VERTEX_SHADER
                            | vk::PipelineStageFlags2::FRAGMENT_SHADER
                            | vk::PipelineStageFlags2::MESH_SHADER_EXT
                            | vk::PipelineStageFlags2::TASK_SHADER_EXT
                            | vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                    }
                    ShaderStage::RayTracing => vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    ShaderStage::RayGeneration => vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    ShaderStage::RayMiss => vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    ShaderStage::RayClosestHit => vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    ShaderStage::RayAnyHit => vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                };

                match &update.value {
                    DescriptorValue::UniformBuffer {
                        buffer,
                        array_element,
                    } => {
                        let sharing_mode = buffer.sharing_mode();
                        let buffer = buffer.internal();

                        buffers.push(
                            vk::DescriptorBufferInfo::default()
                                .buffer(buffer.buffer)
                                .offset(buffer.aligned_size * (*array_element) as u64)
                                .range(buffer.aligned_size),
                        );

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER),
                            buffers.len() - 1..buffers.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::UniformBuffer {
                                _ref_counter: buffer.ref_counter.clone(),
                                buffer: buffer.buffer,
                                id: buffer.id,
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
                        let sharing_mode = buffer.sharing_mode();
                        let buffer = buffer.internal();

                        buffers.push(
                            vk::DescriptorBufferInfo::default()
                                .buffer(buffer.buffer)
                                .offset(buffer.offset(*array_element))
                                .range(buffer.aligned_size),
                        );

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER),
                            buffers.len() - 1..buffers.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::StorageBuffer {
                                _ref_counter: buffer.ref_counter.clone(),
                                buffer: buffer.buffer,
                                id: buffer.id,
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
                        let sharing_mode = texture.sharing_mode();
                        let texture = texture.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::default()
                            .format(texture.format)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: texture.aspect_flags & SAMPLEABLE_ASPECTS,
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
                            .image(texture.image);

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::default()
                                .sampler(sampler_cache.get(&ctx.device, *sampler))
                                .image_view(view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                        );

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
                            images.len() - 1..images.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::Texture {
                                _ref_counter: texture.ref_counter.clone(),
                                image: texture.image,
                                id: texture.id,
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
                        let sharing_mode = texture.sharing_mode();
                        let texture = texture.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::default()
                            .format(texture.format)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: texture.aspect_flags & SAMPLEABLE_ASPECTS,
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
                            .image(texture.image);

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::default()
                                .image_view(view)
                                .image_layout(vk::ImageLayout::GENERAL),
                        );

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE),
                            images.len() - 1..images.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::StorageImage {
                                _ref_counter: texture.ref_counter.clone(),
                                image: texture.image,
                                id: texture.id,
                                view,
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
                        let sharing_mode = cube_map.sharing_mode();
                        let cube_map = cube_map.internal();

                        // Create a view for the texture
                        let create_info = vk::ImageViewCreateInfo::default()
                            .format(cube_map.format)
                            .view_type(vk::ImageViewType::CUBE)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: cube_map.aspect_flags & SAMPLEABLE_ASPECTS,
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
                            .image(cube_map.image);

                        let view = ctx.device.create_image_view(&create_info, None).unwrap();

                        images.push(
                            vk::DescriptorImageInfo::default()
                                .sampler(sampler_cache.get(&ctx.device, *sampler))
                                .image_view(view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                        );

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
                            images.len() - 1..images.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::CubeMap {
                                _ref_counter: cube_map.ref_counter.clone(),
                                image: cube_map.image,
                                id: cube_map.id,
                                view,
                                sharing_mode,
                                aspect_mask: cube_map.aspect_flags,
                                mip_count: *mip_count as u32,
                                base_mip: *base_mip as u32,
                                array_element: *array_element,
                            },
                        }
                    }
                    DescriptorValue::TopLevelAccelerationStructure(tlas) => {
                        tlas_writes.push(
                            vk::WriteDescriptorSetAccelerationStructureKHR::default()
                                .acceleration_structures(std::slice::from_ref(
                                    &tlas.internal().acceleration_struct,
                                )),
                        );
                        let idx = tlas_writes.len() - 1;

                        writes.push((
                            vk::WriteDescriptorSet::default()
                                .dst_set(self.set)
                                .dst_binding(update.binding)
                                .dst_array_element(update.array_element as u32)
                                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR),
                            idx..tlas_writes.len(),
                        ));

                        Binding {
                            access,
                            stage,
                            value: BoundValue::Tlas {
                                _ref_counter: tlas.internal().ref_counter.clone(),
                                buffer: tlas.internal().buffer,
                                id: tlas.internal().id,
                                sharing_mode: tlas.sharing_mode(),
                                aligned_size: tlas.internal().buffer_size as usize,
                            },
                        }
                    }
                }
            });
        }

        // Map temp writes to final writes
        let writes: Vec<_> = writes
            .into_iter()
            .map(|(mut write, range)| match write.descriptor_type {
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER | vk::DescriptorType::STORAGE_IMAGE => {
                    if range.is_empty() {
                        println!("empty");
                    }
                    write.image_info(&images[range])
                }
                vk::DescriptorType::STORAGE_BUFFER | vk::DescriptorType::UNIFORM_BUFFER => {
                    if range.is_empty() {
                        println!("empty");
                    }
                    write.buffer_info(&buffers[range])
                }
                vk::DescriptorType::ACCELERATION_STRUCTURE_KHR => {
                    if range.is_empty() {
                        println!("empty");
                    }
                    write.p_next = <*const _>::cast(&tlas_writes[range.start]);
                    write.descriptor_count = 1;
                    write
                }
                _ => unreachable!("should handle all possible types"),
            })
            .collect();

        ctx.device.update_descriptor_sets(&writes, &[]);
    }
}

impl Drop for DescriptorSet {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::DescriptorSet {
            set: self.set,
            id: self.id,
            layout: self.layout,
            bindings: std::mem::take(&mut self.bound),
        });
    }
}
