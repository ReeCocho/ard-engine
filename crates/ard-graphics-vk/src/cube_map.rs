use ash::vk;
use std::sync::Arc;

use ard_graphics_api::prelude::{CubeMapApi, CubeMapCreateInfo, SamplerDescriptor};

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
    pub image: Arc<Image>,
    pub sampler: SamplerDescriptor,
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
            mip_levels: 1,
            array_layers: 6,
            format: ard_to_vk_format(create_info.format),
        };

        let image = Image::new(&img_create_info);

        // Create view
        let view_create_info = vk::ImageViewCreateInfo::builder()
            .image(image.image())
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(img_create_info.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 6,
            })
            .build();

        let view = ctx
            .0
            .device
            .create_image_view(&view_create_info, None)
            .expect("unable to create cube map image view");

        // Create staging buffer for image data
        let staging_buffer = Buffer::new_staging_buffer(ctx, create_info.data);

        (
            Self {
                image: Arc::new(image),
                sampler: create_info.sampler,
                view,
            },
            staging_buffer,
        )
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
