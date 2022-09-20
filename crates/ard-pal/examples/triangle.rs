/// This example demonstrates how to draw a simple triangle, including the use of staging buffers
/// and the async transfer queue.
use ard_pal::prelude::*;
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[path = "./util.rs"]
mod util;

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Triangle")
        .with_inner_size(PhysicalSize::new(1270, 720))
        .build(&event_loop)
        .unwrap();

    let backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("Triangle"),
        engine_name: String::from("pal"),
        window: &window,
        debug: true,
    })
    .unwrap();

    let context = Context::new(backend);

    let mut surface = Surface::new(
        context.clone(),
        SurfaceCreateInfo {
            config: SurfaceConfiguration {
                width: 1280,
                height: 720,
                present_mode: PresentMode::Fifo,
                format: TextureFormat::Bgra8Unorm,
            },
            window: &window,
            debug_name: Some(String::from("surface")),
        },
    )
    .unwrap();

    // Create triangle buffers
    let buffers = util::create_triangle(&context);
    let vertex_buffer = buffers.vertex;
    let vertex_staging = buffers.vertex_staging;
    let index_buffer = buffers.index;
    let index_staging = buffers.index_staging;

    // Write the staging buffers to the primary buffers
    context
        .transfer()
        .submit(Some("buffer_upload"), |command_buffer| {
            command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
                src: &index_staging,
                src_array_element: 0,
                src_offset: 0,
                dst: &index_buffer,
                dst_array_element: 0,
                dst_offset: 0,
                len: index_buffer.size(),
            });

            command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
                src: &vertex_staging,
                src_array_element: 0,
                src_offset: 0,
                dst: &vertex_buffer,
                dst_array_element: 0,
                dst_offset: 0,
                len: vertex_buffer.size(),
            });
        });
    std::mem::drop(vertex_staging);
    std::mem::drop(index_staging);

    // Compile our shader modules
    let vertex_shader = Shader::new(
        context.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./shaders/triangle.vert.spv"),
            debug_name: Some(String::from("vertex_shader")),
        },
    )
    .unwrap();

    let fragment_shader = Shader::new(
        context.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./shaders/triangle.frag.spv"),
            debug_name: Some(String::from("fragment_shader")),
        },
    )
    .unwrap();

    // Create our graphics pipeline
    let pipeline = GraphicsPipeline::new(
        context.clone(),
        GraphicsPipelineCreateInfo {
            stages: ShaderStages {
                vertex: vertex_shader,
                fragment: Some(fragment_shader),
            },
            layouts: Vec::default(),
            vertex_input: VertexInputState {
                attributes: vec![
                    // This attribute describes the position of our vertex
                    VertexInputAttribute {
                        location: 0,
                        binding: 0,
                        format: VertexFormat::XyzwF32,
                        offset: 0,
                    },
                    // This one describes the color
                    VertexInputAttribute {
                        location: 1,
                        binding: 0,
                        format: VertexFormat::XyzwF32,
                        offset: 16,
                    },
                ],
                bindings: vec![
                    // Our pipeline uses a single buffer containing both the position and color.
                    VertexInputBinding {
                        binding: 0,
                        stride: 32,
                        input_rate: VertexInputRate::Vertex,
                    },
                ],
                topology: PrimitiveTopology::TriangleList,
            },
            rasterization: RasterizationState::default(),
            depth_stencil: None,
            color_blend: Some(ColorBlendState {
                attachments: vec![ColorBlendAttachment {
                    write_mask: ColorComponents::R | ColorComponents::G | ColorComponents::B,
                    ..Default::default()
                }],
            }),
            debug_name: Some(String::from("graphics_pipeline")),
        },
    )
    .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let _ = (&pipeline, &index_buffer, &vertex_buffer, &context, &surface);

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

                let surface_image = surface.acquire_image().unwrap();

                context.main().submit(Some("main_pass"), |command_buffer| {
                    command_buffer.render_pass(
                        RenderPassDescriptor {
                            color_attachments: vec![ColorAttachment {
                                source: ColorAttachmentSource::SurfaceImage(&surface_image),
                                load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                                store_op: StoreOp::Store,
                            }],
                            depth_stencil_attachment: None,
                        },
                        |pass| {
                            // Bind our graphics pipeline
                            pass.bind_pipeline(pipeline.clone());

                            // Bind vertex and index buffers
                            pass.bind_vertex_buffers(
                                0,
                                vec![VertexBind {
                                    buffer: &vertex_buffer,
                                    array_element: 0,
                                    offset: 0,
                                }],
                            );
                            pass.bind_index_buffer(&index_buffer, 0, 0, IndexType::U16);

                            // Draw the triangle
                            pass.draw_indexed(3, 1, 0, 0, 0);
                        },
                    );
                });

                match context.present().present(&surface, surface_image).unwrap() {
                    SurfacePresentSuccess::Ok => {}
                    SurfacePresentSuccess::Invalidated => {
                        let dims = window.inner_size();
                        surface
                            .update_config(SurfaceConfiguration {
                                width: dims.width,
                                height: dims.height,
                                present_mode: PresentMode::Fifo,
                                format: TextureFormat::Bgra8Unorm,
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
