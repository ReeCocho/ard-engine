use ard_pal::prelude::*;
/// This example demonstrates how to use uniform buffers, including writing data directly from the
/// CPU.
use bytemuck::{Pod, Zeroable};
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[repr(C)]
#[derive(Copy, Clone)]
struct UniformData {
    offset: [f32; 2],
}

unsafe impl Zeroable for UniformData {}
unsafe impl Pod for UniformData {}

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Uniform Buffer")
        .with_inner_size(PhysicalSize::new(1270, 720))
        .build(&event_loop)
        .unwrap();

    let backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("Uniform Buffer"),
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
                format: Format::Bgra8Unorm,
            },
            window: &window,
            debug_name: Some(String::from("surface")),
        },
    )
    .unwrap();

    // Create vertex buffer
    const VERTICES: &'static [f32] = &[
        // First
        -0.5, -0.5, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, // Second
        0.5, -0.5, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, // Third
        0.0, 0.5, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0,
    ];
    let vertex_staging = Buffer::new_staging(
        context.clone(),
        QueueType::Main,
        Some(String::from("vertex_staging")),
        bytemuck::cast_slice(&VERTICES),
    )
    .unwrap();

    let vertex_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: (VERTICES.len() * std::mem::size_of::<f32>()) as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
            memory_usage: MemoryUsage::GpuOnly,
            queue_types: QueueTypes::MAIN,
            sharing_mode: SharingMode::Exclusive,
            debug_name: Some(String::from("vertex_buffer")),
        },
    )
    .unwrap();

    // Create index buffer
    const INDEX: &'static [u16] = &[0, 1, 2];
    let index_staging = Buffer::new_staging(
        context.clone(),
        QueueType::Main,
        Some(String::from("index_staging")),
        bytemuck::cast_slice(&INDEX),
    )
    .unwrap();

    let index_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: (INDEX.len() * std::mem::size_of::<u16>()) as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST,
            memory_usage: MemoryUsage::GpuOnly,
            queue_types: QueueTypes::MAIN,
            sharing_mode: SharingMode::Exclusive,
            debug_name: Some(String::from("index_buffer")),
        },
    )
    .unwrap();

    // Write the staging buffers to the primary buffers
    let mut command_buffer = context.main().command_buffer();
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
    context.main().submit(Some("buffer_upload"), command_buffer);

    std::mem::drop(vertex_staging);
    std::mem::drop(index_staging);

    // Create uniform buffer
    let mut uniform_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: std::mem::size_of::<UniformData>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::UNIFORM_BUFFER,
            memory_usage: MemoryUsage::CpuToGpu,
            queue_types: QueueTypes::MAIN,
            sharing_mode: SharingMode::Exclusive,
            debug_name: Some(String::from("uniform_buffer")),
        },
    )
    .unwrap();

    // Create descriptor set layout for our pipeline
    let layout = DescriptorSetLayout::new(
        context.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                binding: 0,
                ty: DescriptorType::UniformBuffer,
                count: 1,
                stage: ShaderStage::Vertex,
            }],
        },
    )
    .unwrap();

    // Create the descriptor set
    let mut set = DescriptorSet::new(
        context.clone(),
        DescriptorSetCreateInfo {
            layout: layout.clone(),
            debug_name: Some(String::from("uniform_compute_set")),
        },
    )
    .unwrap();

    // Bind the uniform buffer to the set
    set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::UniformBuffer {
            buffer: &uniform_buffer,
            array_element: 0,
        },
    }]);

    // Compile our shader modules
    let vertex_shader = Shader::new(
        context.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./shaders/uniform_buffer.vert.spv"),
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
            layouts: vec![layout.clone()],
            vertex_input: VertexInputState {
                attributes: vec![
                    // This attribute describes the position of our vertex
                    VertexInputAttribute {
                        location: 0,
                        binding: 0,
                        format: Format::Rgba32SFloat,
                        offset: 0,
                    },
                    // This one describes the color
                    VertexInputAttribute {
                        location: 1,
                        binding: 0,
                        format: Format::Rgba32SFloat,
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
            color_blend: ColorBlendState {
                attachments: vec![ColorBlendAttachment {
                    write_mask: ColorComponents::R | ColorComponents::G | ColorComponents::B,
                    ..Default::default()
                }],
            },
            push_constants_size: None,
            debug_name: Some(String::from("graphics_pipeline")),
        },
    )
    .unwrap();

    let mut timer: f32 = 0.0;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let _ = (
            &pipeline,
            &index_buffer,
            &vertex_buffer,
            &uniform_buffer,
            &context,
            &surface,
        );

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

                // Update the UBO
                timer += 0.01;
                uniform_buffer
                    .write(0)
                    .unwrap()
                    .copy_from_slice(bytemuck::cast_slice(&[UniformData {
                        offset: [timer.cos() * 0.1, timer.sin() * 0.1],
                    }]));

                let surface_image = surface.acquire_image().unwrap();

                let mut command_buffer = context.main().command_buffer();
                command_buffer.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            source: ColorAttachmentSource::SurfaceImage(&surface_image),
                            load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                            store_op: StoreOp::Store,
                            samples: MultiSamples::Count1,
                        }],
                        color_resolve_attachments: Vec::default(),
                        depth_stencil_attachment: None,
                        depth_stencil_resolve_attachment: None,
                    },
                    None,
                    |pass| {
                        // Bind our graphics pipeline
                        pass.bind_pipeline(pipeline.clone());

                        // Bind our descriptor set
                        pass.bind_sets(0, vec![&set]);

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
                context.main().submit(Some("main_pass"), command_buffer);

                match context.present().present(&surface, surface_image).unwrap() {
                    SurfacePresentSuccess::Ok => {}
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
