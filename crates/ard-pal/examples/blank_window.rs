/// This example demonstrates how to create a blank window and explains all of the objects used.
use api::surface::SurfacePresentSuccess;
use ard_pal::prelude::*;
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Blank Window")
        .with_inner_size(PhysicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    // First, initialize the backend you want to use. This depends on the API, but we're using
    // Vulkan here
    let backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("Blank Window"),
        engine_name: String::from("pal"),
        window: &window,
        debug: true,
    })
    .unwrap();

    // Second, you initialize the context
    let context = Context::new(backend);

    // Third, create a surface with the window
    let mut surface = Surface::new(
        context.clone(),
        SurfaceCreateInfo {
            config: SurfaceConfiguration {
                width: 1280,
                height: 720,
                present_mode: PresentMode::Fifo,
                format: Format::Bgra8Unorm,
            },
            window: &window,
            debug_name: Some(String::from("surface")),
        },
    )
    .unwrap();

    // Finally, begin your event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let _ = (&context, &surface);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => *control_flow = ControlFlow::Exit,
            Event::RedrawRequested(window_id) => {
                let window_size = window.inner_size();
                if window_id != window.id() || window_size.width == 0 || window_size.height == 0 {
                    return;
                }

                // To perform rendering, we begin by asking for an image from our surface
                let surface_image = surface.acquire_image().unwrap();

                // Then we acquire a command buffer from our main queue which supports graphics
                let mut command_buffer = context.main().command_buffer();

                // Begin a render pass to clear the surface image
                command_buffer.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            // The source of our image is from a surface (as opposed to a texture)
                            dst: ColorAttachmentDestination::SurfaceImage(&surface_image),
                            // We clear the image with black
                            load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                            // And store the image instead of discarding the contents
                            store_op: StoreOp::Store,
                            // Only a single sample per fragment.
                            samples: MultiSamples::Count1,
                        }],
                        color_resolve_attachments: Vec::default(),
                        depth_stencil_attachment: None,
                        depth_stencil_resolve_attachment: None,
                    },
                    None,
                    |_pass| {
                        // Here is where you would put rendering commands if you wanted to draw
                        // objects to the screen. We aren't doing that here. If you'd like an
                        // example, refer to `triangle.rs`.
                    },
                );

                // Submit the commands
                context.main().submit(Some("main_pass"), command_buffer);

                // After we're done rendering, we must submit the surface image for presentation
                match context.present().present(&surface, surface_image).unwrap() {
                    // Presentation was successful. No action needed.
                    SurfacePresentSuccess::Ok => {}
                    // Presentation was successful, but the surface has been invalidated. This is
                    // usually do to a resize. All we need to do is update the configuration with
                    // the appropriate configuration
                    SurfacePresentSuccess::Invalidated => {
                        let dims = window.inner_size();
                        surface
                            .update_config(SurfaceConfiguration {
                                width: dims.width,
                                height: dims.height,
                                present_mode: PresentMode::Fifo,
                                format: Format::Bgra8Unorm,
                            })
                            .unwrap();
                    }
                }
            }
            Event::MainEventsCleared => window.request_redraw(),
            _ => {}
        }
    });
}
