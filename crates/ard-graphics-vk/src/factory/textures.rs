use std::collections::HashMap;

use ash::vk;

use super::{container::ResourceContainer, descriptors::DescriptorPool};
use crate::{
    alloc::{Buffer, Image, ImageCreateInfo},
    prelude::*,
    shader_constants::FRAMES_IN_FLIGHT,
};

pub(crate) struct TextureSets {
    ctx: GraphicsContext,
    pool: DescriptorPool,
    _error_image: Image,
    error_image_view: vk::ImageView,
    anisotropy: Option<AnisotropyLevel>,
    sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    new_textures: [Vec<u32>; FRAMES_IN_FLIGHT],
    mip_updates: [Vec<MipUpdate>; FRAMES_IN_FLIGHT],
    dropped_textures: [Vec<u32>; FRAMES_IN_FLIGHT],
    samplers: HashMap<SamplerDescriptor, vk::Sampler>,
}

#[derive(Copy, Clone)]
pub(crate) struct MipUpdate {
    /// ID of the texture that had its mip updated.
    pub id: u32,
    /// Old image view to be dropped.
    pub old_view: vk::ImageView,
    /// Frame index which the old view should be dropped on.
    pub frame_to_drop: usize,
}

const DEFAULT_SAMPLER: SamplerDescriptor = SamplerDescriptor {
    min_filter: TextureFilter::Nearest,
    max_filter: TextureFilter::Nearest,
    mip_filter: TextureFilter::Nearest,
    x_tiling: TextureTiling::ClampToEdge,
    y_tiling: TextureTiling::ClampToEdge,
    anisotropic_filtering: false,
};

impl TextureSets {
    pub unsafe fn new(ctx: &GraphicsContext, anisotropy: Option<AnisotropyLevel>) -> Self {
        let error_image = {
            let create_info = ImageCreateInfo {
                ctx: ctx.clone(),
                width: 1,
                height: 1,
                memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
                image_usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                mip_levels: 1,
                array_layers: 1,
                format: vk::Format::R8G8B8A8_UNORM,
            };

            Image::new(&create_info)
        };

        let error_image_view = {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(error_image.image())
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            ctx.0
                .device
                .create_image_view(&create_info, None)
                .expect("unable to create error image view")
        };

        let magenta = [255u8, 0, 255, 255];
        let staging_buffer = Buffer::new_staging_buffer(ctx, &magenta);

        // Upload staging buffer to error image
        let (command_pool, commands) = ctx
            .0
            .create_single_use_pool(ctx.0.queue_family_indices.transfer);
        super::staging::transition_image_layout(
            &ctx.0.device,
            commands,
            &error_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            0,
            1,
        );
        super::staging::buffer_to_image_copy(
            &ctx.0.device,
            commands,
            &error_image,
            &staging_buffer,
            0,
        );
        super::staging::transition_image_layout(
            &ctx.0.device,
            commands,
            &error_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            0,
            1,
        );
        ctx.0
            .submit_single_use_pool(ctx.0.transfer, command_pool, commands);

        let pool = {
            let bindings = [vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_count(VkBackend::MAX_TEXTURES as u32)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
                .build()];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(ctx, &layout_create_info, FRAMES_IN_FLIGHT)
        };

        // Create initial texture sets
        let sets = [vk::DescriptorSet::default(); FRAMES_IN_FLIGHT];

        let mut textures = Self {
            ctx: ctx.clone(),
            pool,
            _error_image: error_image,
            error_image_view,
            anisotropy,
            sets,
            new_textures: Default::default(),
            dropped_textures: Default::default(),
            mip_updates: Default::default(),
            samplers: HashMap::default(),
        };

        // Fill with error image
        let error_image_info = [vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(error_image_view)
            .sampler(textures.get_sampler(&DEFAULT_SAMPLER))
            .build()];

        let mut writes = Vec::with_capacity(VkBackend::MAX_TEXTURES);
        for i in 0..VkBackend::MAX_TEXTURES {
            writes.push(
                vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_array_element(i as u32)
                    .dst_binding(0)
                    .image_info(&error_image_info)
                    .build(),
            );
        }

        for set in &mut textures.sets {
            *set = textures.pool.allocate();

            for write in &mut writes {
                write.dst_set = *set;
            }

            ctx.0.device.update_descriptor_sets(&writes, &[]);
        }

        textures
    }

    #[inline]
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.pool.layout()
    }

    #[inline]
    pub fn anisotropy(&self) -> Option<AnisotropyLevel> {
        self.anisotropy
    }

    #[inline]
    pub fn get_set(&self, frame: usize) -> vk::DescriptorSet {
        self.sets[frame]
    }

    pub fn get_sampler(&mut self, descriptor: &SamplerDescriptor) -> vk::Sampler {
        // Check if sampler already exists
        if let Some(sampler) = self.samplers.get(descriptor) {
            *sampler
        }
        // If it doesn't, make a new one
        else {
            let create_info = vk::SamplerCreateInfo::builder()
                .address_mode_u(ard_to_vk_tiling(descriptor.x_tiling))
                .address_mode_v(ard_to_vk_tiling(descriptor.y_tiling))
                .mag_filter(ard_to_vk_filter(descriptor.max_filter))
                .min_filter(ard_to_vk_filter(descriptor.min_filter))
                .mipmap_mode(ard_to_vk_mip_mode(descriptor.mip_filter))
                .min_lod(0.0)
                .max_lod(vk::LOD_CLAMP_NONE);

            let create_info = if let Some(anisotropy) = self.anisotropy {
                create_info
                    .anisotropy_enable(true)
                    .max_anisotropy(match anisotropy {
                        AnisotropyLevel::X1 => 1.0,
                        AnisotropyLevel::X2 => 2.0,
                        AnisotropyLevel::X4 => 4.0,
                        AnisotropyLevel::X8 => 8.0,
                        AnisotropyLevel::X16 => 16.0,
                    })
                    .build()
            } else {
                create_info.build()
            };

            let sampler = unsafe {
                self.ctx
                    .0
                    .device
                    .create_sampler(&create_info, None)
                    .expect("unable to create texture sampler")
            };

            self.samplers.insert(*descriptor, sampler);

            sampler
        }
    }

    /// Update the anisotropy level for textures.
    ///
    /// ## Note
    /// This function stalls the GPU while it is performed. This should NOT be called frequently.
    pub fn set_anisotropy(
        &mut self,
        anisotropy: Option<AnisotropyLevel>,
        textures: &ResourceContainer<TextureInner>,
    ) {
        if self.anisotropy == anisotropy {
            return;
        }

        // Wait for all GPU work to finish so we can update all sets without worry
        unsafe {
            self.ctx.0.device.device_wait_idle().unwrap();
        }

        self.anisotropy = anisotropy;

        // Delete samplers that use anisotropy
        // TODO: When drain_filter becomes stable, use that instead of this
        let mut to_delete = Vec::default();
        for descriptor in self.samplers.keys() {
            if descriptor.anisotropic_filtering {
                to_delete.push(*descriptor);
            }
        }

        for descriptor in to_delete {
            unsafe {
                self.ctx
                    .0
                    .device
                    .destroy_sampler(self.samplers.remove(&descriptor).unwrap(), None);
            }
        }

        // Find all textures that use anisotropy and rebind them to the set
        for (id, texture) in textures.resources.iter().enumerate() {
            if let Some(texture) = texture {
                if texture.sampler.anisotropic_filtering {
                    self.texture_ready(id as u32);
                }
            }
        }

        // Update sets for all frames
        for frame in 0..FRAMES_IN_FLIGHT {
            unsafe {
                self.update_sets(frame, textures);
            }
        }
    }

    /// Signal that a texture is uploaded and is ready to be bound to the texture set.
    #[inline]
    pub fn texture_ready(&mut self, id: u32) {
        for list in &mut self.new_textures {
            list.push(id);
        }
    }

    /// Signal that a texture has been dropped and should be removed from the texture set.
    #[inline]
    pub fn texture_dropped(&mut self, id: u32) {
        for list in &mut self.dropped_textures {
            list.push(id);
        }
    }

    /// Signal that a texture has had its mip map updated and a new view needs to be bound.
    #[inline]
    pub fn texture_mip_update(&mut self, update: MipUpdate) {
        for list in &mut self.mip_updates {
            list.push(update);
        }
    }

    /// Binds ready textures to the main set for the given frame and unbinds destroyed textures.
    pub unsafe fn update_sets(&mut self, frame: usize, textures: &ResourceContainer<TextureInner>) {
        let mut img_info =
            Vec::with_capacity(self.new_textures[frame].len() + self.dropped_textures[frame].len());
        let mut writes =
            Vec::with_capacity(self.new_textures[frame].len() + self.dropped_textures[frame].len());

        while let Some(id) = self.new_textures[frame].pop() {
            if let Some(tex) = textures.get(id) {
                img_info.push([vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(tex.view)
                    .sampler(self.get_sampler(&tex.sampler))
                    .build()]);

                writes.push(
                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .dst_array_element(id)
                        .dst_binding(0)
                        .dst_set(self.sets[frame])
                        .image_info(&img_info[img_info.len() - 1])
                        .build(),
                );
            }
        }

        while let Some(mip_update) = self.mip_updates[frame].pop() {
            if let Some(tex) = textures.get(mip_update.id) {
                img_info.push([vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(tex.view)
                    .sampler(self.get_sampler(&tex.sampler))
                    .build()]);

                writes.push(
                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .dst_array_element(mip_update.id)
                        .dst_binding(0)
                        .dst_set(self.sets[frame])
                        .image_info(&img_info[img_info.len() - 1])
                        .build(),
                );
            }

            if frame == mip_update.frame_to_drop {
                self.ctx
                    .0
                    .device
                    .destroy_image_view(mip_update.old_view, None);
            }
        }

        while let Some(id) = self.dropped_textures[frame].pop() {
            img_info.push([vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(self.error_image_view)
                .sampler(self.get_sampler(&DEFAULT_SAMPLER))
                .build()]);

            writes.push(
                vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_array_element(id)
                    .dst_binding(0)
                    .dst_set(self.sets[frame])
                    .image_info(&img_info[img_info.len() - 1])
                    .build(),
            );
        }

        self.ctx.0.device.update_descriptor_sets(&writes, &[]);
    }
}

impl Drop for TextureSets {
    fn drop(&mut self) {
        unsafe {
            for (_, sampler) in self.samplers.drain() {
                self.ctx.0.device.destroy_sampler(sampler, None);
            }

            self.ctx
                .0
                .device
                .destroy_image_view(self.error_image_view, None);
        }
    }
}
