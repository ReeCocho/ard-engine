use std::sync::{Arc, Mutex};

use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_window::{
    prelude::{Window, WindowId},
    windows::Windows,
};
use ash::{extensions::khr, vk};

use crate::prelude::GraphicsContext;

#[derive(Resource, Clone)]
pub struct Surface(pub(crate) Arc<Mutex<SurfaceInner>>);

pub(crate) struct SurfaceInner {
    pub(crate) ctx: GraphicsContext,
    pub(crate) window: WindowId,
    pub(crate) surface: vk::SurfaceKHR,
    pub(crate) format: vk::SurfaceFormatKHR,
    pub(crate) resolution: vk::Extent2D,
    pub(crate) swapchain: vk::SwapchainKHR,
    pub(crate) images: Vec<vk::Image>,
    /// Semaphores for image availability
    semaphores: Vec<vk::Semaphore>,
    /// Rolling index for semaphore acquisition.
    next_semaphore: usize,
    surface_loader: khr::Surface,
    swapchain_loader: khr::Swapchain,
}

impl Surface {
    pub(crate) unsafe fn from_raw(
        ctx: &GraphicsContext,
        window: &Window,
        surface: vk::SurfaceKHR,
        surface_loader: khr::Surface,
    ) -> Self {
        let format = pick_best_surface_format(ctx, &surface_loader, surface)
            .expect("no compatible surface formats");

        let swapchain_loader = khr::Swapchain::new(&ctx.0.instance, &ctx.0.device);

        let mut inner = SurfaceInner {
            ctx: ctx.clone(),
            window: window.id(),
            surface,
            format,
            resolution: vk::Extent2D::default(),
            surface_loader,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::default(),
            semaphores: Vec::default(),
            next_semaphore: 0,
        };

        inner.regenerate_swapchain(window.physical_width(), window.physical_height());

        Surface(Arc::new(Mutex::new(inner)))
    }
}

impl SurfaceInner {
    /// Acquires a new swapchain image. Returns the index of the image and the semaphore to wait on
    /// for image availability.
    pub(crate) unsafe fn acquire_image(&mut self) -> (usize, vk::Semaphore) {
        let semaphore = self.semaphores[self.next_semaphore];
        let img_idx = self
            .swapchain_loader
            .acquire_next_image(self.swapchain, u64::MAX, semaphore, vk::Fence::null())
            .expect("unable to acquire swapchain image")
            .0 as usize;
        self.next_semaphore = (self.next_semaphore + 1) % self.semaphores.len();
        (img_idx, semaphore)
    }

    /// Submit the image for presentation given semaphores to wait on and a fence to signal.
    /// Returns `true` if the swapchain was invalidated and needed to be regenerated.
    pub(crate) unsafe fn present(
        &mut self,
        image_idx: usize,
        waits: &[vk::Semaphore],
        windows: &Windows,
    ) -> bool {
        let swapchain = [self.swapchain];
        let image_idx = [image_idx as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(waits)
            .swapchains(&swapchain)
            .image_indices(&image_idx)
            .build();

        self.swapchain_loader
            .queue_present(self.ctx.0.present, &present_info)
            .unwrap_or_else(|_| unsafe {
                self.ctx.0.device.device_wait_idle().unwrap();
                let window = windows
                    .get(self.window)
                    .expect("surface points to invalid window");
                let width = window.physical_width();
                let height = window.physical_height();
                self.regenerate_swapchain(width, height);
                true
            })
    }

    /// Recreates the swapchain with the new width and height.
    pub(crate) unsafe fn regenerate_swapchain(&mut self, width: u32, height: u32) {
        assert!(width > 0 && height > 0);

        // Destroy old swapchain
        self.release_swapchain();

        let surface_capabilities = self
            .surface_loader
            .get_physical_device_surface_capabilities(self.ctx.0.physical_device, self.surface)
            .expect("unable to get surface capabilities of device");

        // Choose number of images
        let mut desired_image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && desired_image_count > surface_capabilities.max_image_count
        {
            desired_image_count = surface_capabilities.max_image_count;
        }

        // Choose swapchain size based on provided dimensions
        let surface_resolution = vk::Extent2D {
            width: std::cmp::min(width, surface_capabilities.max_image_extent.width),
            height: std::cmp::min(height, surface_capabilities.max_image_extent.height),
        };

        // No transformation preferred
        let pre_transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };

        // Create the swapchain
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(self.surface)
            .min_image_count(desired_image_count)
            .image_color_space(self.format.color_space)
            .image_format(self.format.format)
            .image_extent(surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            // TODO: Custom present modes
            .present_mode(vk::PresentModeKHR::IMMEDIATE)
            .clipped(true)
            .image_array_layers(1);

        let swapchain = self
            .swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .expect("unable to create swapchain");

        // Get swapchain images
        self.images = self
            .swapchain_loader
            .get_swapchain_images(swapchain)
            .expect("unable to get swapchain images");

        // Create image availability semaphores
        self.semaphores = Vec::with_capacity(self.images.len());
        for _ in 0..self.images.len() {
            let create_info = vk::SemaphoreCreateInfo::default();
            self.semaphores.push(
                self.ctx
                    .0
                    .device
                    .create_semaphore(&create_info, None)
                    .expect("unable to create semaphore"),
            );
        }

        self.next_semaphore = 0;
        self.resolution = vk::Extent2D { width, height };
        self.swapchain = swapchain;
    }

    pub(crate) unsafe fn release_swapchain(&mut self) {
        for semaphore in self.semaphores.drain(..) {
            self.ctx.0.device.destroy_semaphore(semaphore, None);
        }

        if self.swapchain != vk::SwapchainKHR::null() {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

impl Drop for SurfaceInner {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.device.device_wait_idle().unwrap();
            self.release_swapchain();
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}

impl SurfaceApi for Surface {}

unsafe fn pick_best_surface_format(
    ctx: &GraphicsContext,
    loader: &khr::Surface,
    surface: vk::SurfaceKHR,
) -> Option<vk::SurfaceFormatKHR> {
    let surface_formats =
        match loader.get_physical_device_surface_formats(ctx.0.physical_device, surface) {
            Ok(formats) => formats,
            Err(_) => return None,
        };

    // Current best format and precedence
    let mut final_format = None;
    let mut precedence = u32::MAX;

    for format in surface_formats {
        let new_precedence = match format.format {
            vk::Format::B8G8R8A8_SRGB => 0,
            vk::Format::R8G8B8A8_SRGB => 1,
            vk::Format::B8G8R8A8_UNORM => 2,
            vk::Format::R8G8B8A8_UNORM => 3,
            // No-op for unsupported formats
            _ => continue,
        };

        if new_precedence < precedence {
            precedence = new_precedence;
            final_format = Some(format);
        }
    }

    final_format
}
