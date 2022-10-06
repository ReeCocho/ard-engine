use std::{ffi::CString, mem::ManuallyDrop, sync::Arc};

use api::cube_map::{CubeMapCreateError, CubeMapCreateInfo};
use ash::vk::{self, Handle};
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, Allocator};

use crate::{texture::TextureRefCounter, util::garbage_collector::Garbage};

pub struct CubeMap {
    pub(crate) image: vk::Image,
    /// Image view for each array element and mip level. This array is flattened like so.
    /// A0M0 -> A0M1 -> A0M2 ... A1M0 -> A1M1 -> A1M2 -> ...
    pub(crate) views: Vec<vk::ImageView>,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) ref_counter: TextureRefCounter,
    pub(crate) format: vk::Format,
    pub(crate) mip_count: u32,
    pub(crate) aspect_flags: vk::ImageAspectFlags,
    on_drop: Sender<Garbage>,
}

impl CubeMap {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        on_drop: Sender<Garbage>,
        allocator: &mut Allocator,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self, CubeMapCreateError> {
        // Create the image
        let format = crate::util::to_vk_format(create_info.format);
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: create_info.size,
                height: create_info.size,
                depth: 1,
            })
            .mip_levels(create_info.mip_levels as u32)
            .array_layers(6 * create_info.array_elements as u32)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(crate::util::to_vk_image_usage(create_info.texture_usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::CUBE_COMPATIBLE)
            .build();

        let image = match device.create_image(&image_create_info, None) {
            Ok(image) => image,
            Err(err) => return Err(CubeMapCreateError::Other(err.to_string())),
        };

        // Determine memory requirements
        let mem_reqs = device.get_image_memory_requirements(image);

        // Allocate memory
        let request = AllocationCreateDesc {
            name: match &create_info.debug_name {
                Some(name) => &name,
                None => "image",
            },
            requirements: mem_reqs,
            location: crate::util::to_gpu_allocator_memory_location(create_info.memory_usage),
            linear: false,
        };

        let block = match allocator.allocate(&request) {
            Ok(block) => block,
            Err(err) => {
                device.destroy_image(image, None);
                return Err(CubeMapCreateError::Other(err.to_string()));
            }
        };

        // Bind image to memory
        if let Err(err) = device.bind_image_memory(image, block.memory(), block.offset()) {
            allocator.free(block).unwrap();
            device.destroy_image(image, None);
            return Err(CubeMapCreateError::Other(err.to_string()));
        }

        // Create views
        let mut views = Vec::with_capacity(create_info.array_elements * create_info.mip_levels);
        let aspect_flags = if create_info.format.is_color() {
            vk::ImageAspectFlags::COLOR
        } else {
            vk::ImageAspectFlags::DEPTH
                | if create_info.format.is_stencil() {
                    vk::ImageAspectFlags::STENCIL
                } else {
                    vk::ImageAspectFlags::empty()
                }
        };
        for i in 0..create_info.array_elements {
            for j in 0..create_info.mip_levels {
                let view_create_info = vk::ImageViewCreateInfo::builder()
                    .format(format)
                    .view_type(vk::ImageViewType::CUBE)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: 6 * i as u32,
                        layer_count: 6,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image)
                    .build();
                views.push(device.create_image_view(&view_create_info, None).unwrap());
            }
        }

        // Setup debug name is requested
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = debug {
                let cstr_name = CString::new(name.as_str()).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::IMAGE)
                    .object_handle(image.as_raw())
                    .object_name(&cstr_name)
                    .build();

                debug
                    .debug_utils_set_object_name(device.handle(), &name_info)
                    .unwrap();

                for (i, view) in views.iter().enumerate() {
                    let name = CString::new(format!("{}_view_{}", &name, i)).unwrap();
                    let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                        .object_type(vk::ObjectType::IMAGE_VIEW)
                        .object_handle(view.as_raw())
                        .object_name(&name)
                        .build();

                    debug
                        .debug_utils_set_object_name(device.handle(), &name_info)
                        .unwrap();
                }
            }
        }

        Ok(CubeMap {
            image,
            views,
            block: ManuallyDrop::new(block),
            on_drop,
            ref_counter: TextureRefCounter::default(),
            format,
            mip_count: create_info.mip_levels as u32,
            aspect_flags,
        })
    }
}

impl Drop for CubeMap {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::Texture {
            image: self.image,
            views: std::mem::take(&mut self.views),
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
        });
    }
}