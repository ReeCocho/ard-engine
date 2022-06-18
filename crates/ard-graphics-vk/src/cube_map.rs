use ash::vk;
use std::sync::Arc;

use ard_graphics_api::prelude::{CubeMapApi, CubeMapCreateInfo, MipType, SamplerDescriptor};

use crate::{
    alloc::{Buffer, Image, ImageCreateInfo},
    camera::{ard_to_vk_format, container::EscapeHandle, GraphicsContext},
};

#[derive(Clone)]
pub struct CubeMap {
    pub(crate) id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct CubeMapInner {
    pub image: Image,
    pub sampler: SamplerDescriptor,
    pub mip_levels: u32,
    /// Bit mask that indicates which mip levels of the texture are loaded into memory. The least
    /// significant bit represents LOD0 (the highest detail image).
    pub loaded_mips: u32,
    pub view: vk::ImageView,
}

impl CubeMapInner {
    pub unsafe fn new(ctx: &GraphicsContext, create_info: &CubeMapCreateInfo) -> (Self, Buffer) {
        assert_ne!(create_info.width, 0);
        assert_ne!(create_info.height, 0);

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
            array_layers: 6,
            format: ard_to_vk_format(create_info.format),
            flags: vk::ImageCreateFlags::CUBE_COMPATIBLE,
        };

        let image = Image::new(&img_create_info);

        // Create view
        let view_create_info = vk::ImageViewCreateInfo::builder()
            .image(image.image())
            .view_type(vk::ImageViewType::CUBE)
            .format(img_create_info.format);

        let view_create_info = match create_info.mip_type {
            MipType::Upload => view_create_info
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: create_info.mip_count as u32 - 1,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 6,
                })
                .build(),
            _ => view_create_info
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: create_info.mip_count as u32,
                    base_array_layer: 0,
                    layer_count: 6,
                })
                .build(),
        };

        let view = ctx
            .0
            .device
            .create_image_view(&view_create_info, None)
            .expect("unable to create cube map image view");

        // Create staging buffer for image data
        let staging_buffer = Buffer::new_staging_buffer(ctx, create_info.data);

        let loaded_mips = match create_info.mip_type {
            MipType::Generate => (1 << create_info.mip_count) - 1,
            MipType::Upload => (1 << (create_info.mip_count - 1)),
        };

        (
            Self {
                image,
                sampler: create_info.sampler,
                view,
                mip_levels: create_info.mip_count as u32,
                loaded_mips,
            },
            staging_buffer,
        )
    }

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
            .view_type(vk::ImageViewType::CUBE)
            .format(self.image.format())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level,
                level_count: loaded_mips,
                base_array_layer: 0,
                layer_count: 6,
            })
            .build();

        let mut view = device
            .create_image_view(&create_info, None)
            .expect("unable to create texture image view");
        std::mem::swap(&mut view, &mut self.view);
        view
    }
}

impl Drop for CubeMapInner {
    fn drop(&mut self) {
        unsafe {
            self.image.ctx.0.device.destroy_image_view(self.view, None);
        }
    }
}

impl CubeMapApi for CubeMap {}
