use std::{ffi::CString, mem::ManuallyDrop, sync::Arc};

use crate::{util::garbage_collector::Garbage, QueueFamilyIndices};
use api::{
    texture::{TextureCreateError, TextureCreateInfo},
    types::*,
};
use ash::vk::{self, Handle};
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};

pub struct Texture {
    pub(crate) image: vk::Image,
    /// Image view for each array element and mip level. This array is flattened like so.
    /// A0M0 -> A0M1 -> A0M2 ... A1M0 -> A1M1 -> A1M2 -> ...
    pub(crate) views: Vec<vk::ImageView>,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) _image_usage: TextureUsage,
    pub(crate) _memory_usage: MemoryUsage,
    pub(crate) _array_elements: usize,
    pub(crate) size: u64,
    pub(crate) ref_counter: TextureRefCounter,
    pub(crate) format: vk::Format,
    pub(crate) mip_count: u32,
    pub(crate) aspect_flags: vk::ImageAspectFlags,
    on_drop: Sender<Garbage>,
}

#[derive(Clone)]
pub(crate) struct TextureRefCounter(Arc<()>);

impl Texture {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        _qfi: &QueueFamilyIndices,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        on_drop: Sender<Garbage>,
        allocator: &mut Allocator,
        create_info: TextureCreateInfo,
    ) -> Result<Self, TextureCreateError> {
        // Create the image
        let format = crate::util::to_vk_format(create_info.format);
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(crate::util::to_vk_image_type(create_info.ty))
            .extent(vk::Extent3D {
                width: create_info.width,
                height: create_info.height,
                depth: create_info.depth,
            })
            .mip_levels(create_info.mip_levels as u32)
            .array_layers(create_info.array_elements as u32)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(crate::util::to_vk_image_usage(create_info.texture_usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty())
            .build();

        let image = match device.create_image(&image_create_info, None) {
            Ok(image) => image,
            Err(err) => return Err(TextureCreateError::Other(err.to_string())),
        };

        // Determine memory requirements
        let mem_reqs = device.get_image_memory_requirements(image);

        // Allocate memory
        let request = AllocationCreateDesc {
            name: match &create_info.debug_name {
                Some(name) => name,
                None => "image",
            },
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            requirements: mem_reqs,
            location: crate::util::to_gpu_allocator_memory_location(create_info.memory_usage),
            linear: false,
        };

        let block = match allocator.allocate(&request) {
            Ok(block) => block,
            Err(err) => {
                device.destroy_image(image, None);
                return Err(TextureCreateError::Other(err.to_string()));
            }
        };

        // Bind image to memory
        if let Err(err) = device.bind_image_memory(image, block.memory(), block.offset()) {
            allocator.free(block).unwrap();
            device.destroy_image(image, None);
            return Err(TextureCreateError::Other(err.to_string()));
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
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: i as u32,
                        layer_count: 1,
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
                    .set_debug_utils_object_name(device.handle(), &name_info)
                    .unwrap();

                for (i, view) in views.iter().enumerate() {
                    let name = CString::new(format!("{}_view_{}", &name, i)).unwrap();
                    let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                        .object_type(vk::ObjectType::IMAGE_VIEW)
                        .object_handle(view.as_raw())
                        .object_name(&name)
                        .build();

                    debug
                        .set_debug_utils_object_name(device.handle(), &name_info)
                        .unwrap();
                }
            }
        }

        Ok(Texture {
            image,
            views,
            block: ManuallyDrop::new(block),
            _image_usage: create_info.texture_usage,
            _memory_usage: create_info.memory_usage,
            _array_elements: create_info.array_elements,
            size: mem_reqs.size / create_info.array_elements as u64,
            on_drop,
            ref_counter: TextureRefCounter::default(),
            format,
            aspect_flags,
            mip_count: create_info.mip_levels as u32,
        })
    }

    #[inline(always)]
    pub(crate) fn get_view(&self, array_elem: usize, mip: usize) -> vk::ImageView {
        self.views[(array_elem * self.mip_count as usize) + mip]
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::Texture {
            image: self.image,
            views: std::mem::take(&mut self.views),
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
        });
    }
}

impl TextureRefCounter {
    #[inline]
    pub fn is_last(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

impl Default for TextureRefCounter {
    #[inline]
    fn default() -> Self {
        TextureRefCounter(Arc::new(()))
    }
}
