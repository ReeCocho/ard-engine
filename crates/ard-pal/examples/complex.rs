/// This example is meant to act as a complex use case scenario. This is not a "real" program, in
/// the sense that the operations performed make little sense. This simply demonstrates how many
/// of the features fit together.
use ard_pal::prelude::*;
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Complex")
        .with_inner_size(PhysicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    let backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("Complex"),
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

    // Create buffers
    let mut color_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            /// Three vertices, each of four components.
            size: 3 * 4 * std::mem::size_of::<f32>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::TRANSFER_SRC,
            memory_usage: MemoryUsage::CpuToGpu,
            debug_name: Some(String::from("color_buffer")),
        },
    )
    .unwrap();

    let vertex_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            /// Three vertices, each of four components. Positions and colors means two attributes.
            size: 3 * 4 * 2 * std::mem::size_of::<f32>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::TRANSFER_DST
                | BufferUsage::VERTEX_BUFFER
                | BufferUsage::STORAGE_BUFFER,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("vertex_buffer")),
        },
    )
    .unwrap();

    let index_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: 3 * std::mem::size_of::<u32>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::TRANSFER_DST | BufferUsage::INDEX_BUFFER,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("index_buffer")),
        },
    )
    .unwrap();

    let index_buffer_intermediate = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: 3 * std::mem::size_of::<u32>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::TRANSFER_SRC | BufferUsage::STORAGE_BUFFER,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("index_buffer_intermediate")),
        },
    )
    .unwrap();

    // Compute shader to generate vertex positions
    let vertex_compute_shader = Shader::new(
        context.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./shaders/vertex_compute.comp.spv"),
            debug_name: Some(String::from("vertex_compute_shader")),
        },
    )
    .unwrap();

    let compute_layout = DescriptorSetLayout::new(
        context.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                binding: 0,
                count: 1,
                stage: ShaderStage::Compute,
            }],
        },
    )
    .unwrap();

    let mut vertex_compute_set = DescriptorSet::new(
        context.clone(),
        DescriptorSetCreateInfo {
            layout: compute_layout.clone(),
            debug_name: Some(String::from("vertex_compute_set")),
        },
    )
    .unwrap();

    vertex_compute_set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::StorageBuffer {
            buffer: &vertex_buffer,
            array_element: 0,
        },
    }]);

    let vertex_compute_pipeline = ComputePipeline::new(
        context.clone(),
        ComputePipelineCreateInfo {
            layouts: vec![compute_layout.clone()],
            module: vertex_compute_shader,
            work_group_size: (3, 1, 1),
            push_constants_size: None,
            debug_name: Some(String::from("vertex_compute_pipeline")),
        },
    )
    .unwrap();

    // Compute shader to generate index positions
    let index_compute_shader = Shader::new(
        context.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./shaders/index_compute.comp.spv"),
            debug_name: Some(String::from("index_compute_shader")),
        },
    )
    .unwrap();

    let mut index_compute_set = DescriptorSet::new(
        context.clone(),
        DescriptorSetCreateInfo {
            layout: compute_layout.clone(),
            debug_name: Some(String::from("index_compute_set")),
        },
    )
    .unwrap();

    index_compute_set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::StorageBuffer {
            buffer: &index_buffer_intermediate,
            array_element: 0,
        },
    }]);

    let index_compute_pipeline = ComputePipeline::new(
        context.clone(),
        ComputePipelineCreateInfo {
            layouts: vec![compute_layout.clone()],
            module: index_compute_shader,
            work_group_size: (3, 1, 1),
            push_constants_size: None,
            debug_name: Some(String::from("index_compute_pipeline")),
        },
    )
    .unwrap();

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
    let graphics_pipeline = GraphicsPipeline::new(
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
                        offset: 16 * 3,
                    },
                ],
                bindings: vec![
                    // Our pipeline uses a single buffer containing both the position and color.
                    VertexInputBinding {
                        binding: 0,
                        stride: 16,
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
            push_constants_size: None,
            debug_name: Some(String::from("graphics_pipeline")),
        },
    )
    .unwrap();

    let mut timer: f32 = 0.0;
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

                // First, we write the colors to the color buffer on the CPU (this requires a
                // GPU to CPU sync from the last frame because we must guarantee the buffer is
                // not being accessed by the GPU when we write the data).
                timer += 0.01;
                let colors = [
                    // First vertex
                    timer.cos().powi(2),
                    (timer + std::f32::consts::FRAC_PI_3).cos().powi(2),
                    (timer + (2.0 * std::f32::consts::FRAC_PI_3)).cos().powi(2),
                    1.0,
                    // Second vertex
                    (timer + std::f32::consts::FRAC_PI_3).cos().powi(2),
                    timer.cos().powi(2),
                    (timer + (2.0 * std::f32::consts::FRAC_PI_3)).cos().powi(2),
                    1.0,
                    // Third vertex
                    (timer + (2.0 * std::f32::consts::FRAC_PI_3)).cos().powi(2),
                    timer.cos().powi(2),
                    (timer + std::f32::consts::FRAC_PI_3).cos().powi(2),
                    1.0,
                ];
                color_buffer
                    .write(0)
                    .unwrap()
                    .copy_from_slice(bytemuck::cast_slice(&colors));

                // Next, we use a transfer command to copy the color buffer to the vertex buffer
                // (this requires a GPU to GPU sync because we must gaurantee the buffer is not
                // being accessed by another queue).
                let mut command_buffer = context.transfer().command_buffer();
                command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
                    src: &color_buffer,
                    src_array_element: 0,
                    src_offset: 0,
                    dst: &vertex_buffer,
                    dst_array_element: 0,
                    // Skip the position vertices and write to the colors
                    dst_offset: 3 * 4 * std::mem::size_of::<f32>() as u64,
                    len: color_buffer.size(),
                });
                context
                    .transfer()
                    .submit(Some("color_buffer_copy"), command_buffer);

                // Next, we use a compute shader to generate the vertex positions (this requires a
                // GPU to GPU sync with the transfer command because we can't write until the
                // buffer is done being written to).
                let mut command_buffer = context.compute().command_buffer();
                command_buffer.compute_pass(|pass| {
                    pass.bind_pipeline(vertex_compute_pipeline.clone());
                    pass.bind_sets(0, vec![&vertex_compute_set]);
                    pass.dispatch(1, 1, 1);
                });
                context
                    .compute()
                    .submit(Some("vertex_compute"), command_buffer);

                // Finally, we have a pass with three steps.
                // 1. Generate our index buffers using a compute shader.
                // 2. Copy the index data to the actual index buffer using a transfer operation
                // 3. Use a render pass to draw the generated vertex and index buffers
                //
                // This demonstrates GPU to GPU sync with the previous two queue operations and
                // also command synchronization because each operation depends on the last.
                let surface_image = surface.acquire_image().unwrap();
                let mut command_buffer = context.main().command_buffer();

                // 1. Generate indices
                command_buffer.compute_pass(|pass| {
                    pass.bind_pipeline(index_compute_pipeline.clone());
                    pass.bind_sets(0, vec![&index_compute_set]);
                    pass.dispatch(1, 1, 1);
                });

                // 2. Copy indices
                command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
                    src: &index_buffer_intermediate,
                    src_array_element: 0,
                    src_offset: 0,
                    dst: &index_buffer,
                    dst_array_element: 0,
                    dst_offset: 0,
                    len: index_buffer_intermediate.size(),
                });

                // 3. Render everything
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
                        pass.bind_pipeline(graphics_pipeline.clone());
                        pass.bind_vertex_buffers(
                            0,
                            vec![VertexBind {
                                buffer: &vertex_buffer,
                                array_element: 0,
                                offset: 0,
                            }],
                        );
                        pass.bind_index_buffer(&index_buffer, 0, 0, IndexType::U32);
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
