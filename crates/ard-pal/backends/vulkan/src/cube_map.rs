use std::{ffi::CString, mem::ManuallyDrop};

use api::{
    cube_map::{CubeMapCreateError, CubeMapCreateInfo},
    types::{CubeFace, SharingMode},
};
use ash::vk;
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};

use crate::{
    texture::TextureRefCounter,
    util::{
        cube_face_to_idx,
        garbage_collector::Garbage,
        id_gen::{IdGenerator, ResourceId},
    },
    QueueFamilyIndices,
};

pub struct CubeMap {
    pub(crate) image: vk::Image,
    pub(crate) id: ResourceId,
    /// Image view for each array element and mip level. This array is flattened like so.
    /// A0M0 -> A0M1 -> A0M2 ... A1M0 -> A1M1 -> A1M2 -> ...
    pub(crate) views: Vec<vk::ImageView>,
    /// Image view for each array element, each mip level, and each face. This array is flattened
    /// like `views` but, the third "dimension" is the cube face as in `cube_face_to_idx`.
    pub(crate) face_views: Vec<vk::ImageView>,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) ref_counter: TextureRefCounter,
    pub(crate) format: vk::Format,
    pub(crate) mip_count: u32,
    pub(crate) _array_elements: usize,
    pub(crate) aspect_flags: vk::ImageAspectFlags,
    pub(crate) sharing_mode: SharingMode,
    pub(crate) size: u64,
    on_drop: Sender<Garbage>,
}

impl CubeMap {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        id_gen: &IdGenerator,
        qfi: &QueueFamilyIndices,
        debug: Option<&ash::ext::debug_utils::Device>,
        on_drop: Sender<Garbage>,
        allocator: &mut Allocator,
        create_info: CubeMapCreateInfo,
    ) -> Result<Self, CubeMapCreateError> {
        // Create the image
        let format = crate::util::to_vk_format(create_info.format);
        let qfi = qfi.queue_types_to_indices(create_info.queue_types);
        let image_create_info = vk::ImageCreateInfo::default()
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
            .sharing_mode(if qfi.len() == 1 {
                vk::SharingMode::EXCLUSIVE
            } else {
                crate::util::to_vk_sharing_mode(create_info.sharing_mode)
            })
            .queue_family_indices(&qfi)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::CUBE_COMPATIBLE);

        let image = match device.create_image(&image_create_info, None) {
            Ok(image) => image,
            Err(err) => return Err(CubeMapCreateError::Other(err.to_string())),
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
                let view_create_info = vk::ImageViewCreateInfo::default()
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
                    .image(image);
                views.push(device.create_image_view(&view_create_info, None).unwrap());
            }
        }

        // Create face views
        let mut face_views = Vec::with_capacity(views.len() * 6);
        for i in 0..create_info.array_elements {
            for j in 0..create_info.mip_levels {
                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::East)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());

                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::West)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());

                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::Top)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());

                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::Bottom)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());

                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::North)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());

                let view_create_info = vk::ImageViewCreateInfo::default()
                    .format(format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect_flags,
                        base_mip_level: j as u32,
                        level_count: 1,
                        base_array_layer: ((6 * i) + cube_face_to_idx(CubeFace::South)) as u32,
                        layer_count: 1,
                    })
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .image(image);
                face_views.push(device.create_image_view(&view_create_info, None).unwrap());
            }
        }

        // Setup debug name is requested
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = debug {
                let cstr_name = CString::new(name.as_str()).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(image)
                    .object_name(&cstr_name);

                debug.set_debug_utils_object_name(&name_info).unwrap();

                for (i, view) in views.iter().enumerate() {
                    let name = CString::new(format!("{}_view_{}", &name, i)).unwrap();
                    let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                        .object_handle(*view)
                        .object_name(&name);

                    debug.set_debug_utils_object_name(&name_info).unwrap();
                }
            }
        }

        Ok(CubeMap {
            image,
            id: id_gen.create(),
            views,
            face_views,
            block: ManuallyDrop::new(block),
            sharing_mode: create_info.sharing_mode,
            on_drop,
            ref_counter: TextureRefCounter::default(),
            format,
            mip_count: create_info.mip_levels as u32,
            _array_elements: create_info.array_elements,
            size: mem_reqs.size / create_info.array_elements as u64,
            aspect_flags,
        })
    }

    #[inline(always)]
    pub(crate) fn to_array_elem(array_elem: usize, face: CubeFace) -> usize {
        (array_elem * 6) + cube_face_to_idx(face)
    }

    #[inline(always)]
    pub(crate) fn get_face_view(
        &self,
        array_elem: usize,
        mip: usize,
        face: CubeFace,
    ) -> vk::ImageView {
        self.face_views
            [(array_elem * 6 * self.mip_count as usize) + (mip * 6) + cube_face_to_idx(face)]
    }

    #[inline(always)]
    pub(crate) fn get_view(&self, array_elem: usize, mip: usize) -> vk::ImageView {
        self.views[(array_elem * 6) + mip]
    }
}

impl Drop for CubeMap {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::Texture {
            image: self.image,
            id: self.id,
            views: {
                let face_views = std::mem::take(&mut self.face_views);
                let mut views = std::mem::take(&mut self.views);
                views.extend(face_views);
                views
            },
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
        });
    }
}
