use std::{
    borrow::Cow,
    ffi::{c_void, CStr, CString},
    mem::ManuallyDrop,
    sync::{Arc, Mutex},
};

use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_window::windows::Windows;
use ard_winit::windows::WinitWindows;
use ash::{
    extensions::{ext, khr},
    vk::{self, DebugUtilsMessageSeverityFlagsEXT},
    Entry,
};
use gpu_allocator::vulkan::*;

use crate::{
    alloc::{Buffer, Image, ImageCreateInfo},
    surface::Surface,
    VkBackend,
};

#[derive(Resource, Clone)]
pub struct GraphicsContext(pub(crate) Arc<GraphicsContextInner>);

pub(crate) struct GraphicsContextInner {
    pub _entry: ash::Entry,
    pub instance: ash::Instance,
    pub debug: Option<(ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
    pub properties: vk::PhysicalDeviceProperties,
    pub features: vk::PhysicalDeviceFeatures,
    pub device: Arc<ash::Device>,
    pub main: vk::Queue,
    pub transfer: vk::Queue,
    pub present: vk::Queue,
    pub compute: vk::Queue,
    pub allocator: ManuallyDrop<Mutex<Allocator>>,
}

#[derive(Default)]
pub(crate) struct QueueFamilyIndices {
    /// Must support graphics, transfer, and compute.
    pub main: u32,
    /// Must support presentation.
    pub present: u32,
    /// Must support transfer.
    pub transfer: u32,
    /// Must support compute.
    pub compute: u32,
    pub unique: Vec<u32>,
}

struct PhysicalDeviceQuery {
    pub device: vk::PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
    pub properties: vk::PhysicalDeviceProperties,
    pub features: vk::PhysicalDeviceFeatures,
}

impl GraphicsContextApi<VkBackend> for GraphicsContext {
    fn new(
        resources: &Resources,
        create_info: &GraphicsContextCreateInfo,
    ) -> Result<(Self, Surface), GraphicsContextCreateError> {
        let windows = resources.get::<Windows>().unwrap();
        let winit_windows = resources.get::<WinitWindows>().unwrap();
        let window = windows
            .get(create_info.window)
            .expect("graphics context expected window that did not exist");
        let winit_window = winit_windows
            .get_window(create_info.window)
            .expect("graphics context expected window that did not exist");

        let app_name = CString::new(window.title()).unwrap();

        let layer_names = if create_info.debug {
            vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()]
        } else {
            Vec::default()
        };
        let layers_names_raw: Vec<*const i8> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        let surface_extensions = ash_window::enumerate_required_extensions(winit_window).unwrap();
        let mut extension_names_raw = surface_extensions
            .iter()
            .map(|ext| *ext)
            .collect::<Vec<_>>();

        if create_info.debug {
            extension_names_raw.push(ext::DebugUtils::name().as_ptr());
        }

        let mut device_extensions = vec![CString::from(khr::Swapchain::name())];
        if create_info.debug {
            device_extensions.push(CString::new("VK_KHR_shader_non_semantic_info").unwrap());
            // device_extensions.push(CString::from(khr::Synchronization2::name()));
        }

        let device_extension_names_raw = device_extensions
            .iter()
            .map(|ext| ext.as_ptr() as *const i8)
            .collect::<Vec<_>>();

        let vk_version = vk::make_api_version(0, 1, 2, 0);

        unsafe {
            let entry = match Entry::load() {
                Ok(entry) => entry,
                Err(err) => return Err(GraphicsContextCreateError(err.to_string())),
            };

            let appinfo = vk::ApplicationInfo::builder()
                .application_name(&app_name)
                .application_version(0)
                .engine_name(&app_name)
                .engine_version(0)
                .api_version(vk_version);

            let instance_create_info = vk::InstanceCreateInfo::builder()
                .application_info(&appinfo)
                .enabled_layer_names(&layers_names_raw)
                .enabled_extension_names(&extension_names_raw);

            let instance = match entry.create_instance(&instance_create_info, None) {
                Ok(instance) => instance,
                Err(err) => return Err(GraphicsContextCreateError(err.to_string())),
            };

            let debug = if create_info.debug {
                let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                    .message_severity(
                        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                            | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                            | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                    )
                    .message_type(
                        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                            | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                            | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                    )
                    .pfn_user_callback(Some(vulkan_debug_callback));

                let debug_utils_loader = ext::DebugUtils::new(&entry, &instance);

                let debug_messenger =
                    match debug_utils_loader.create_debug_utils_messenger(&debug_info, None) {
                        Ok(messenger) => messenger,
                        Err(err) => return Err(GraphicsContextCreateError(err.to_string())),
                    };

                Some((debug_utils_loader, debug_messenger))
            } else {
                None
            };

            let surface = match ash_window::create_surface(&entry, &instance, winit_window, None) {
                Ok(surface) => surface,
                Err(err) => return Err(GraphicsContextCreateError(err.to_string())),
            };
            let surface_loader = khr::Surface::new(&entry, &instance);

            let pd_query = match pick_physical_device(&instance, surface, &surface_loader, &[]) {
                Some(pd) => pd,
                None => {
                    return Err(GraphicsContextCreateError(String::from(
                        "could not find a suitable physical device",
                    )))
                }
            };

            let mut priorities = Vec::with_capacity(pd_query.queue_family_indices.unique.len());
            let mut queue_infos = Vec::with_capacity(pd_query.queue_family_indices.unique.len());
            let mut queue_indices = (0, 0, 0, 0);
            for q in &pd_query.queue_family_indices.unique {
                let mut cur_priorities = Vec::with_capacity(4);

                if pd_query.queue_family_indices.main == *q {
                    queue_indices.0 = cur_priorities.len();
                    cur_priorities.push(1.0);
                }

                if pd_query.queue_family_indices.transfer == *q {
                    queue_indices.1 = cur_priorities.len();
                    cur_priorities.push(1.0);
                }

                if pd_query.queue_family_indices.present == *q {
                    queue_indices.2 = cur_priorities.len();
                    cur_priorities.push(1.0);
                }

                if pd_query.queue_family_indices.compute == *q {
                    queue_indices.3 = cur_priorities.len();
                    cur_priorities.push(1.0);
                }

                queue_infos.push(
                    vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(*q)
                        .queue_priorities(&cur_priorities)
                        .build(),
                );

                priorities.push(cur_priorities);
            }

            let features = vk::PhysicalDeviceFeatures::builder()
                .fill_mode_non_solid(true)
                .draw_indirect_first_instance(true)
                .multi_draw_indirect(true)
                .depth_clamp(true)
                .build();

            let mut indexing_features = vk::PhysicalDeviceDescriptorIndexingFeatures::builder()
                .runtime_descriptor_array(true)
                .build();

            let mut buffer_device_addr = vk::PhysicalDeviceBufferDeviceAddressFeatures::builder()
                .buffer_device_address(true)
                .build();
            buffer_device_addr.p_next = std::ptr::addr_of_mut!(indexing_features) as *mut c_void;

            let mut features2 = vk::PhysicalDeviceFeatures2::builder()
                .push_next(&mut buffer_device_addr)
                .features(features)
                .build();

            let create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&device_extension_names_raw)
                .push_next(&mut features2)
                .build();

            let device = match instance.create_device(pd_query.device, &create_info, None) {
                Ok(device) => device,
                Err(err) => return Err(GraphicsContextCreateError(err.to_string())),
            };

            let main =
                device.get_device_queue(pd_query.queue_family_indices.main, queue_indices.0 as u32);
            let transfer = device.get_device_queue(
                pd_query.queue_family_indices.transfer,
                queue_indices.1 as u32,
            );
            let present = device.get_device_queue(
                pd_query.queue_family_indices.present,
                queue_indices.2 as u32,
            );
            let compute = device.get_device_queue(
                pd_query.queue_family_indices.compute,
                queue_indices.3 as u32,
            );

            let allocator = ManuallyDrop::new(Mutex::new(
                Allocator::new(&AllocatorCreateDesc {
                    instance: instance.clone(),
                    device: device.clone(),
                    physical_device: pd_query.device,
                    debug_settings: gpu_allocator::AllocatorDebugSettings {
                        log_memory_information: false,
                        log_leaks_on_shutdown: true,
                        store_stack_traces: false,
                        log_allocations: false,
                        log_frees: false,
                        log_stack_traces: false,
                    },
                    // TODO: Look into this
                    buffer_device_address: false,
                })
                .expect("unable to create GPU memory allocator"),
            ));

            let inner = Arc::new(GraphicsContextInner {
                _entry: entry,
                instance,
                debug,
                physical_device: pd_query.device,
                properties: pd_query.properties,
                features: pd_query.features,
                queue_family_indices: pd_query.queue_family_indices,
                device: Arc::new(device),
                main,
                transfer,
                present,
                compute,
                allocator,
            });
            let ctx = GraphicsContext(inner);

            let surface = Surface::from_raw(&ctx, window, surface, surface_loader);

            Ok((ctx, surface))
        }
    }
}

impl GraphicsContext {
    /// Initializes an image and view from some data source.
    ///
    /// # Note
    /// This should ONLY be used during initialization functions, or when stalling the rendering
    /// pipeline is required. Using this WILL stall the entire GPU while work is performed.
    pub(crate) unsafe fn create_image(
        &self,
        data: &[u8],
        dims: (u32, u32, u32),
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        final_layout: vk::ImageLayout,
    ) -> (Image, vk::ImageView) {
        let image = {
            let create_info = ImageCreateInfo {
                ctx: self.clone(),
                ty: if dims.2 == 1 {
                    vk::ImageType::TYPE_2D
                } else {
                    vk::ImageType::TYPE_3D
                },
                width: dims.0,
                height: dims.1,
                depth: dims.2,
                memory_usage: gpu_allocator::MemoryLocation::GpuOnly,
                image_usage: usage | vk::ImageUsageFlags::TRANSFER_DST,
                mip_levels: 1,
                array_layers: 1,
                format,
                flags: vk::ImageCreateFlags::empty(),
            };

            Image::new(&create_info)
        };

        let view = {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(image.image())
                .view_type(if dims.2 == 1 {
                    vk::ImageViewType::TYPE_2D
                } else {
                    vk::ImageViewType::TYPE_3D
                })
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            self.0
                .device
                .create_image_view(&create_info, None)
                .expect("unable to create image view")
        };

        let staging_buffer = Buffer::new_staging_buffer(self, data);

        // Upload staging buffer to error image
        let (command_pool, commands) = self
            .0
            .create_single_use_pool(self.0.queue_family_indices.transfer);

        let barrier = [vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
            .image(image.image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build()];

        self.0.device.cmd_pipeline_barrier(
            commands,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::BY_REGION,
            &[],
            &[],
            &barrier,
        );

        let regions = [vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: dims.0,
                height: dims.1,
                depth: dims.2,
            })
            .build()];

        self.0.device.cmd_copy_buffer_to_image(
            commands,
            staging_buffer.buffer(),
            image.image(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
        );

        let barrier = [vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(final_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .image(image.image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build()];

        self.0.device.cmd_pipeline_barrier(
            commands,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::BY_REGION,
            &[],
            &[],
            &barrier,
        );

        self.0
            .submit_single_use_pool(self.0.transfer, command_pool, commands);

        (image, view)
    }
}

impl GraphicsContextInner {
    /// Allocates a command pool and command buffer to be used for single time events.
    ///
    /// # Note
    /// This should ONLY be used during initialization functions, or when stalling the rendering
    /// pipeline is required. Using this WILL stall the entire GPU while work is performed.
    pub unsafe fn create_single_use_pool(
        &self,
        family_index: u32,
    ) -> (vk::CommandPool, vk::CommandBuffer) {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(family_index)
            .build();

        let pool = self
            .device
            .create_command_pool(&create_info, None)
            .expect("unable to create single use command pool");

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .build();

        let command_buffer = self
            .device
            .allocate_command_buffers(&alloc_info)
            .expect("unable to allocate single use command buffer")[0];

        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();

        self.device
            .begin_command_buffer(command_buffer, &begin_info)
            .expect("unable to begin single use command buffer");

        (pool, command_buffer)
    }

    /// Submits a single use command and waits for it to complete.
    ///
    /// # Note
    /// This should ONLY be used during initialization functions, or when stalling the rendering
    /// pipeline is required. Using this WILL stall the entire GPU while work is performed.
    pub unsafe fn submit_single_use_pool(
        &self,
        queue: vk::Queue,
        pool: vk::CommandPool,
        command_buffer: vk::CommandBuffer,
    ) {
        self.device
            .end_command_buffer(command_buffer)
            .expect("unable to end single use command buffer");

        let command_buffer = [command_buffer];

        let submit = [vk::SubmitInfo::builder()
            .command_buffers(&command_buffer)
            .build()];

        self.device
            .queue_submit(queue, &submit, vk::Fence::null())
            .expect("unable to submit single use commands");
        self.device
            .queue_wait_idle(queue)
            .expect("unable to wait on queue for single use commands");

        self.device.destroy_command_pool(pool, None);
    }
}

impl Drop for GraphicsContextInner {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            std::mem::drop(ManuallyDrop::take(&mut self.allocator));

            self.device.destroy_device(None);
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

impl QueueFamilyIndices {
    // Returns `None` if we can't fill out all queue family types.
    fn find(
        instance: &ash::Instance,
        device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
        surface_loader: &khr::Surface,
    ) -> Option<QueueFamilyIndices> {
        let mut properties =
            unsafe { instance.get_physical_device_queue_family_properties(device) };
        let mut main = usize::MAX;
        let mut present = usize::MAX;
        let mut transfer = usize::MAX;
        let mut compute = usize::MAX;

        // Find main queue. Probably will end up being family 0.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && family.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && family.queue_flags.contains(vk::QueueFlags::COMPUTE)
            {
                main = family_idx;
                break;
            }
        }

        if main == usize::MAX {
            return None;
        }

        properties[main].queue_count -= 1;

        // Find presentation queue. Would be nice to be different from main.
        for (family_idx, _) in properties.iter().enumerate() {
            let surface_support = unsafe {
                match surface_loader.get_physical_device_surface_support(
                    device,
                    family_idx as u32,
                    surface,
                ) {
                    Ok(support) => support,
                    Err(_) => return None,
                }
            };

            if surface_support && properties[family_idx].queue_count > 0 {
                present = family_idx;
                if family_idx != main {
                    break;
                }
            }
        }

        if present == usize::MAX {
            return None;
        }

        properties[present].queue_count -= 1;

        // Look for a dedicated transfer queue. Supported on some devices. Fallback is main.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && properties[family_idx].queue_count > 0
            {
                transfer = family_idx;
                if family_idx != main && family_idx != present {
                    break;
                }
            }
        }

        if transfer == usize::MAX {
            return None;
        }

        properties[transfer].queue_count -= 1;

        // Look for a dedicated async compute queue. Supported on some devices. Fallback is main.
        for (family_idx, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::COMPUTE)
                && properties[family_idx].queue_count > 0
            {
                compute = family_idx;
                if family_idx != main && family_idx != present && family_idx != transfer {
                    break;
                }
            }
        }

        if compute == usize::MAX {
            return None;
        }

        let unique = {
            let mut qfi_set = std::collections::HashSet::<usize>::new();
            qfi_set.insert(main);
            qfi_set.insert(present);
            qfi_set.insert(transfer);
            qfi_set.insert(compute);

            let mut unique_qfis = Vec::with_capacity(qfi_set.len());
            for q in qfi_set {
                unique_qfis.push(q as u32);
            }

            unique_qfis
        };

        Some(QueueFamilyIndices {
            main: main as u32,
            present: present as u32,
            transfer: transfer as u32,
            compute: compute as u32,
            unique,
        })
    }
}

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface: vk::SurfaceKHR,
    loader: &khr::Surface,
    extensions: &[CString],
) -> Option<PhysicalDeviceQuery> {
    let devices = match instance.enumerate_physical_devices() {
        Ok(devices) => devices,
        Err(_) => return None,
    };

    let mut device_type = vk::PhysicalDeviceType::OTHER;
    let mut query = None;
    for device in devices {
        let properties = instance.get_physical_device_properties(device);
        let features = instance.get_physical_device_features(device);

        // Must support requested extensions
        if check_device_extensions(instance, device, extensions).is_some() {
            continue;
        }

        // Must support surface stuff
        let formats = match loader.get_physical_device_surface_formats(device, surface) {
            Ok(formats) => formats,
            Err(_) => continue,
        };

        let present_modes = match loader.get_physical_device_surface_present_modes(device, surface)
        {
            Ok(modes) => modes,
            Err(_) => continue,
        };

        if formats.is_empty() || present_modes.is_empty() {
            continue;
        }

        // Must support all queue family indices
        let qfi = QueueFamilyIndices::find(instance, device, surface, loader);
        if qfi.is_none() {
            continue;
        }

        // Pick this device if it's better than the old one
        if device_type_rank(properties.device_type) >= device_type_rank(device_type) {
            device_type = properties.device_type;
            query = Some(PhysicalDeviceQuery {
                device,
                features,
                properties,
                queue_family_indices: qfi.unwrap(),
            });
        }
    }

    query
}

/// Check that a physical devices supports required device extensions.
///
/// Returns `None` on a success, or `Some` containing the name of the missing extension.
unsafe fn check_device_extensions(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
    extensions: &[CString],
) -> Option<String> {
    let found_extensions = match instance.enumerate_device_extension_properties(device) {
        Ok(extensions) => extensions,
        Err(_) => return Some(String::default()),
    };

    for extension_name in extensions {
        let mut found = false;
        for extension_property in &found_extensions {
            // I know this is slow and dumb but `check_device_extensions` is only called once, so whatever
            let s = CString::from(CStr::from_ptr(extension_property.extension_name.as_ptr()));

            if *extension_name == s {
                found = true;
                break;
            }
        }

        if !found {
            return Some(String::from(extension_name.to_string_lossy()));
        }
    }

    None
}

fn device_type_rank(ty: vk::PhysicalDeviceType) -> u32 {
    match ty {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::CPU => 2,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
        _ => 0,
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number: i32 = callback_data.message_id_number as i32;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    match message_severity {
        DebugUtilsMessageSeverityFlagsEXT::VERBOSE => ard_log::info!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::INFO => ard_log::info!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::WARNING => ard_log::warn!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        DebugUtilsMessageSeverityFlagsEXT::ERROR => ard_log::error!(
            "{:?}:\n{:?} [{} ({})] : {}\n",
            message_severity,
            message_type,
            message_id_name,
            &message_id_number.to_string(),
            message,
        ),
        _ => {}
    }

    vk::FALSE
}
