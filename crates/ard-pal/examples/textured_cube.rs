/// This example demonstrates the use of textures in render passes and as sampled images
/// in shaders.
///
/// To do this, we first render a triangle to an image. Then, we sample that image in a second
/// pass and place it onto a cube.

#[path = "./util.rs"]
mod util;

use ard_pal::prelude::*;
use glam::{Mat4, Vec3};
use ordered_float::NotNan;
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const TRIANGLE_VERTEX_BIN: &'static [u8] = include_bytes!("./shaders/triangle.vert.spv");
const TRIANGLE_FRAGMENT_BIN: &'static [u8] = include_bytes!("./shaders/triangle.frag.spv");
const CUBE_VERTEX_BIN: &'static [u8] = include_bytes!("./shaders/cube.vert.spv");
const CUBE_FRAGMENT_BIN: &'static [u8] = include_bytes!("./shaders/cube.frag.spv");

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Textured Cube")
        .with_inner_size(PhysicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    let backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("Textured Cube"),
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

    // Create and upload our triangle and cube buffers
    let buffers = util::create_triangle(&context);
    let triangle_vertex_buffer = buffers.vertex;
    let triangle_vertex_staging = buffers.vertex_staging;
    let triangle_index_buffer = buffers.index;
    let triangle_index_staging = buffers.index_staging;

    let buffers = util::create_cube(&context);
    let cube_vertex_buffer = buffers.vertex;
    let cube_vertex_staging = buffers.vertex_staging;
    let cube_index_buffer = buffers.index;
    let cube_index_staging = buffers.index_staging;

    let mut command_buffer = context.transfer().command_buffer();
    command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
        src: &triangle_index_staging,
        src_array_element: 0,
        src_offset: 0,
        dst: &triangle_index_buffer,
        dst_array_element: 0,
        dst_offset: 0,
        len: triangle_index_buffer.size(),
    });
    command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
        src: &triangle_vertex_staging,
        src_array_element: 0,
        src_offset: 0,
        dst: &triangle_vertex_buffer,
        dst_array_element: 0,
        dst_offset: 0,
        len: triangle_vertex_buffer.size(),
    });
    command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
        src: &cube_index_staging,
        src_array_element: 0,
        src_offset: 0,
        dst: &cube_index_buffer,
        dst_array_element: 0,
        dst_offset: 0,
        len: cube_index_buffer.size(),
    });
    command_buffer.copy_buffer_to_buffer(CopyBufferToBuffer {
        src: &cube_vertex_staging,
        src_array_element: 0,
        src_offset: 0,
        dst: &cube_vertex_buffer,
        dst_array_element: 0,
        dst_offset: 0,
        len: cube_vertex_buffer.size(),
    });
    context
        .transfer()
        .submit(Some("staging_upload"), command_buffer);

    std::mem::drop(triangle_vertex_staging);
    std::mem::drop(triangle_index_staging);
    std::mem::drop(cube_vertex_staging);
    std::mem::drop(cube_index_staging);

    // Create the pipeline to render our triangle
    let triangle_pipeline = {
        let vertex_shader = Shader::new(
            context.clone(),
            ShaderCreateInfo {
                code: TRIANGLE_VERTEX_BIN,
                debug_name: Some(String::from("triangle_vertex_shader")),
            },
        )
        .unwrap();

        let fragment_shader = Shader::new(
            context.clone(),
            ShaderCreateInfo {
                code: TRIANGLE_FRAGMENT_BIN,
                debug_name: Some(String::from("triangle_fragment_shader")),
            },
        )
        .unwrap();

        GraphicsPipeline::new(
            context.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vertex_shader,
                    fragment: Some(fragment_shader),
                },
                layouts: Vec::default(),
                vertex_input: VertexInputState {
                    attributes: vec![
                        VertexInputAttribute {
                            location: 0,
                            binding: 0,
                            format: Format::Rgba32SFloat,
                            offset: 0,
                        },
                        VertexInputAttribute {
                            location: 1,
                            binding: 0,
                            format: Format::Rgba32SFloat,
                            offset: 16,
                        },
                    ],
                    bindings: vec![VertexInputBinding {
                        binding: 0,
                        stride: 32,
                        input_rate: VertexInputRate::Vertex,
                    }],
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
                debug_name: Some(String::from("triangle_graphics_pipeline")),
            },
        )
        .unwrap()
    };

    // This texture is used to render the triangle
    let triangle_texture = Texture::new(
        context.clone(),
        TextureCreateInfo {
            format: Format::Rgba8Unorm,
            ty: TextureType::Type2D,
            width: 512,
            height: 512,
            depth: 1,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("triangle_texture")),
        },
    )
    .unwrap();

    let uniform_buffer = Buffer::new(
        context.clone(),
        BufferCreateInfo {
            size: std::mem::size_of::<Mat4>() as u64,
            array_elements: 1,
            buffer_usage: BufferUsage::UNIFORM_BUFFER,
            memory_usage: MemoryUsage::CpuToGpu,
            debug_name: Some(String::from("uniform_buffer")),
        },
    )
    .unwrap();

    // Create the pipeline and descriptor set to render our cube
    let (cube_pipeline, cube_set) = {
        let layout = DescriptorSetLayout::new(
            context.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    DescriptorBinding {
                        binding: 0,
                        count: 1,
                        stage: ShaderStage::Fragment,
                        ty: DescriptorType::Texture,
                    },
                    DescriptorBinding {
                        binding: 1,
                        count: 1,
                        stage: ShaderStage::Vertex,
                        ty: DescriptorType::UniformBuffer,
                    },
                ],
            },
        )
        .unwrap();

        let mut set = DescriptorSet::new(
            context.clone(),
            DescriptorSetCreateInfo {
                layout: layout.clone(),
                debug_name: Some(String::from("cube_set")),
            },
        )
        .unwrap();

        // Bind the triangle texture and UBO to the cube set
        set.update(&[
            DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &triangle_texture,
                    array_element: 0,
                    sampler: Sampler {
                        min_filter: Filter::Linear,
                        mag_filter: Filter::Linear,
                        mipmap_filter: Filter::Linear,
                        address_u: SamplerAddressMode::ClampToEdge,
                        address_v: SamplerAddressMode::ClampToEdge,
                        address_w: SamplerAddressMode::ClampToEdge,
                        anisotropy: None,
                        compare: None,
                        min_lod: NotNan::new(0.0).unwrap(),
                        max_lod: None,
                        unnormalize_coords: false,
                        border_color: None,
                    },
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: 1,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: &uniform_buffer,
                    array_element: 0,
                },
            },
        ]);

        // Create the cube pipeline
        let vertex_shader = Shader::new(
            context.clone(),
            ShaderCreateInfo {
                code: CUBE_VERTEX_BIN,
                debug_name: Some(String::from("cube_vertex_shader")),
            },
        )
        .unwrap();

        let fragment_shader = Shader::new(
            context.clone(),
            ShaderCreateInfo {
                code: CUBE_FRAGMENT_BIN,
                debug_name: Some(String::from("cube_fragment_shader")),
            },
        )
        .unwrap();

        let pipeline = GraphicsPipeline::new(
            context.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vertex_shader,
                    fragment: Some(fragment_shader),
                },
                layouts: vec![layout],
                vertex_input: VertexInputState {
                    attributes: vec![
                        VertexInputAttribute {
                            location: 0,
                            binding: 0,
                            format: Format::Rgba32SFloat,
                            offset: 0,
                        },
                        VertexInputAttribute {
                            location: 1,
                            binding: 0,
                            format: Format::Rg32SFloat,
                            offset: 16,
                        },
                    ],
                    bindings: vec![VertexInputBinding {
                        binding: 0,
                        stride: 24,
                        input_rate: VertexInputRate::Vertex,
                    }],
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    cull_mode: CullMode::None,
                    ..Default::default()
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: true,
                    depth_write: true,
                    depth_compare: CompareOp::Less,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        write_mask: ColorComponents::R | ColorComponents::G | ColorComponents::B,
                        ..Default::default()
                    }],
                },
                push_constants_size: None,
                debug_name: Some(String::from("cube_graphics_pipeline")),
            },
        )
        .unwrap();

        (pipeline, set)
    };

    // This texture is used as the depth buffer for the final pass
    let depth_buffer = Texture::new(
        context.clone(),
        TextureCreateInfo {
            format: Format::D16Unorm,
            ty: TextureType::Type2D,
            width: surface.dimensions().0,
            height: surface.dimensions().1,
            depth: 1,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("depth_buffer")),
        },
    )
    .unwrap();

    let mut timer: f32 = 0.0;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let _ = (
            &context,
            &surface,
            &depth_buffer,
            &triangle_texture,
            &triangle_index_buffer,
            &triangle_vertex_buffer,
            &cube_index_buffer,
            &cube_vertex_buffer,
            &uniform_buffer,
            &cube_pipeline,
            &cube_set,
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

                // Write UBO
                timer += 0.01;
                let mvp = Mat4::perspective_rh(
                    80.0_f32.to_radians(),
                    window_size.width as f32 / window_size.height as f32,
                    0.3,
                    50.0,
                );
                let mvp = mvp
                    * Mat4::look_at_rh(
                        Vec3::new(timer.cos() * 3.0, 1.0, timer.sin() * 3.0),
                        Vec3::ZERO,
                        Vec3::Y,
                    );
                uniform_buffer
                    .write(0)
                    .unwrap()
                    .copy_from_slice(bytemuck::cast_slice(&[mvp]));

                // Begin rendering
                let surface_image = surface.acquire_image().unwrap();
                let mut command_buffer = context.main().command_buffer();

                // First pass, we render a triangle to the texture
                command_buffer.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            source: ColorAttachmentSource::Texture {
                                texture: &triangle_texture,
                                array_element: 0,
                                mip_level: 0,
                            },
                            load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                            store_op: StoreOp::Store,
                        }],
                        depth_stencil_attachment: None,
                    },
                    |pass| {
                        pass.bind_pipeline(triangle_pipeline.clone());
                        pass.bind_index_buffer(&triangle_index_buffer, 0, 0, IndexType::U16);
                        pass.bind_vertex_buffers(
                            0,
                            vec![VertexBind {
                                buffer: &triangle_vertex_buffer,
                                array_element: 0,
                                offset: 0,
                            }],
                        );
                        pass.draw_indexed(3, 1, 0, 0, 0);
                    },
                );

                // In the second pass, we sample from the rendered texture and draw a cube
                command_buffer.render_pass(
                    RenderPassDescriptor {
                        color_attachments: vec![ColorAttachment {
                            source: ColorAttachmentSource::SurfaceImage(&surface_image),
                            load_op: LoadOp::Clear(ClearColor::RgbaF32(0.2, 0.2, 0.2, 0.0)),
                            store_op: StoreOp::Store,
                        }],
                        depth_stencil_attachment: Some(DepthStencilAttachment {
                            texture: &depth_buffer,
                            array_element: 0,
                            mip_level: 0,
                            load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                            store_op: StoreOp::DontCare,
                        }),
                    },
                    |pass| {
                        pass.bind_pipeline(cube_pipeline.clone());
                        pass.bind_sets(0, vec![&cube_set]);
                        pass.bind_index_buffer(&cube_index_buffer, 0, 0, IndexType::U16);
                        pass.bind_vertex_buffers(
                            0,
                            vec![VertexBind {
                                buffer: &cube_vertex_buffer,
                                array_element: 0,
                                offset: 0,
                            }],
                        );
                        pass.draw_indexed(36, 1, 0, 0, 0);
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
