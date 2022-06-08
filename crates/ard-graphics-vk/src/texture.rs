use std::sync::Arc;

use ash::vk;

use crate::alloc::{Buffer, Image, ImageCreateInfo};
use crate::factory::container::*;
use crate::prelude::*;

#[derive(Clone)]
pub struct Texture {
    pub(crate) id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct TextureInner {
    pub image: Arc<Image>,
    pub sampler: SamplerDescriptor,
    pub mip_levels: u32,
    /// Bit mask that indicates which mip levels of the texture are loaded into memory. The least
    /// significant bit represents LOD0 (the highest detail image).
    pub loaded_mips: u32,
    pub view: vk::ImageView,
}

impl TextureInner {
    pub unsafe fn new(ctx: &GraphicsContext, create_info: &TextureCreateInfo) -> (Self, Buffer) {
        assert_ne!(create_info.width, 0);
        assert_ne!(create_info.height, 0);

        let mip_levels = (create_info.width.max(create_info.height) as f32)
            .log2()
            .floor() as usize
            + 1;

        assert!(create_info.mip_count <= mip_levels);

        // Create image
        let img_create_info = ImageCreateInfo {
            ctx: ctx.clone(),
            ty: vk::ImageType::TYPE_2D,
            width: create_info.width,
            height: create_info.height,
            depth: 1,
            memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
            image_usage: vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
            mip_levels: create_info.mip_count as u32,
            array_layers: 1,
            format: ard_to_vk_format(create_info.format),
            flags: vk::ImageCreateFlags::empty(),
        };

        let image = Image::new(&img_create_info);

        // Create view
        let view_create_info = vk::ImageViewCreateInfo::builder()
            .image(image.image())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(img_create_info.format);

        let view_create_info = match create_info.mip_type {
            MipType::Upload => view_create_info
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: create_info.mip_count as u32 - 1,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build(),
            _ => view_create_info
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: create_info.mip_count as u32,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build(),
        };

        let view = ctx
            .0
            .device
            .create_image_view(&view_create_info, None)
            .expect("unable to create texture image view");

        // Create staging buffer for image data
        let staging_buffer = Buffer::new_staging_buffer(ctx, create_info.data);

        let loaded_mips = match create_info.mip_type {
            MipType::Generate => (1 << create_info.mip_count) - 1,
            MipType::Upload => (1 << (create_info.mip_count - 1)),
        };

        (
            Self {
                image: Arc::new(image),
                sampler: create_info.sampler,
                mip_levels: create_info.mip_count as u32,
                view,
                loaded_mips,
            },
            staging_buffer,
        )
    }
}

impl TextureApi for Texture {}

impl TextureInner {
    /// Creates a new image view based on the number of loaded mips and return the old view.
    pub unsafe fn create_new_view(&mut self, device: &ash::Device) -> vk::ImageView {
        // Determine how many consecutive mips are ready, starting from the least
        // detailed level
        let mut loaded_mips = self.loaded_mips << (u32::BITS - self.mip_levels);
        let lz = loaded_mips.leading_zeros();
        let loaded_mips = (loaded_mips << lz).leading_ones();
        let base_mip_level = self.mip_levels - (lz + loaded_mips);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(self.image.image())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(self.image.format())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level,
                level_count: loaded_mips,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        let mut view = device
            .create_image_view(&create_info, None)
            .expect("unable to create texture image view");
        std::mem::swap(&mut view, &mut self.view);
        view
    }
}

impl Drop for TextureInner {
    fn drop(&mut self) {
        unsafe {
            self.image.ctx.0.device.destroy_image_view(self.view, None);
        }
    }
}

#[inline]
pub(crate) fn ard_to_vk_format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::R8G8B8A8Srgb => vk::Format::R8G8B8A8_SRGB,
        TextureFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::R16G16B16A16Unorm => vk::Format::R16G16B16A16_UNORM,
        TextureFormat::R32G32B32A32Sfloat => vk::Format::R32G32B32A32_SFLOAT,
        TextureFormat::R8G8Unorm => vk::Format::R8G8_UNORM,
    }
}

#[inline]
pub(crate) fn ard_to_vk_filter(filter: TextureFilter) -> vk::Filter {
    match filter {
        TextureFilter::Linear => vk::Filter::LINEAR,
        TextureFilter::Nearest => vk::Filter::NEAREST,
    }
}

#[inline]
pub(crate) fn ard_to_vk_mip_mode(filter: TextureFilter) -> vk::SamplerMipmapMode {
    match filter {
        TextureFilter::Linear => vk::SamplerMipmapMode::LINEAR,
        TextureFilter::Nearest => vk::SamplerMipmapMode::NEAREST,
    }
}

#[inline]
pub(crate) fn ard_to_vk_tiling(tiling: TextureTiling) -> vk::SamplerAddressMode {
    match tiling {
        TextureTiling::Repeat => vk::SamplerAddressMode::REPEAT,
        TextureTiling::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        TextureTiling::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
    }
}
