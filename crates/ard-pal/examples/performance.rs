use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use ard_pal::prelude::*;
use vulkan::{VulkanBackend, VulkanBackendCreateInfo};
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder};

const TEST1_RUN_COUNT: usize = 10000;
const TEST1_BUFFER_COUNT: usize = 1024;
const TEST1_BUFFER_SIZE: u64 = 128;

fn main() {
    println!("Initializing Pal...");
    // We don't really care about initialization performance, so we preinitialize both wgpu and pal
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Textured Cube")
        .with_inner_size(PhysicalSize::new(1280, 720))
        .with_visible(false)
        .build(&event_loop)
        .unwrap();

    let pal_backend = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("performance_test"),
        engine_name: String::from("pal"),
        window: &window,
        debug: false,
    })
    .unwrap();
    let pal = Context::new(pal_backend);

    println!("Initializing Wgpu...");
    let wgpu = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let wgpu_surface = unsafe { wgpu.create_surface(&window).unwrap() };
    let wgpu_adapter =
        futures::executor::block_on(wgpu.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&wgpu_surface),
            force_fallback_adapter: false,
        }))
        .unwrap();
    let (wgpu_device, wgpu_queue) = futures::executor::block_on(wgpu_adapter.request_device(
        &wgpu::DeviceDescriptor {
            features: wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY
                | wgpu::Features::BUFFER_BINDING_ARRAY,
            limits: wgpu::Limits {
                max_storage_buffers_per_shader_stage: TEST1_BUFFER_COUNT as u32,
                ..Default::default()
            },
            label: None,
        },
        None,
    ))
    .unwrap();

    // Test 1:
    // This test checks the speed of verifying buffer access for many buffers in a compute shader.
    println!("Beginning Test 1...");
    let mut pal_time = Duration::ZERO;
    let mut wgpu_time = Duration::ZERO;

    let mut pal_buffers = Vec::with_capacity(TEST1_BUFFER_COUNT);
    for _ in 0..TEST1_BUFFER_COUNT {
        pal_buffers.push(
            Buffer::new(
                pal.clone(),
                BufferCreateInfo {
                    size: TEST1_BUFFER_SIZE,
                    array_elements: 1,
                    buffer_usage: BufferUsage::UNIFORM_BUFFER,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: None,
                },
            )
            .unwrap(),
        );
    }

    let mut wgpu_buffers = Vec::with_capacity(TEST1_BUFFER_COUNT);
    for _ in 0..TEST1_BUFFER_COUNT {
        wgpu_buffers.push(wgpu_device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: TEST1_BUFFER_SIZE,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        }));
    }

    for _ in 0..TEST1_RUN_COUNT {
        pal_time += pal_test1(&pal, &pal_buffers);
        wgpu_time += wgpu_test1(&wgpu_device, &wgpu_queue, &wgpu_buffers);
    }
    println!(
        "pal_test1 : {} ms",
        pal_time.as_millis() as f32 / TEST1_RUN_COUNT as f32
    );
    println!(
        "wgpu_test1 : {} ms",
        wgpu_time.as_millis() as f32 / TEST1_RUN_COUNT as f32
    );

    std::mem::drop(pal_buffers);

    // Test 2:
    // This test checks the speed of verifying texture access for many textures in a render pass.
    // TODO
}

fn pal_test1(pal: &Context, buffers: &[Buffer]) -> Duration {
    let begin = Instant::now();

    let layout = DescriptorSetLayout::new(
        pal.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                binding: 0,
                ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                count: TEST1_BUFFER_COUNT,
                stage: ShaderStage::Compute,
            }],
        },
    )
    .unwrap();

    let mut set = DescriptorSet::new(
        pal.clone(),
        DescriptorSetCreateInfo {
            layout: layout.clone(),
            debug_name: None,
        },
    )
    .unwrap();

    let mut updates = Vec::with_capacity(TEST1_BUFFER_COUNT);
    for (i, buffer) in buffers.iter().enumerate() {
        updates.push(DescriptorSetUpdate {
            binding: 0,
            array_element: i,
            value: DescriptorValue::UniformBuffer {
                buffer,
                array_element: 1,
            },
        });
    }
    set.update(&updates);

    const SHADER_BIN: &'static [u8] = include_bytes!("./shaders/test1_pal.comp.spv");
    let shader = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: SHADER_BIN,
            debug_name: None,
        },
    )
    .unwrap();

    let pipeline = ComputePipeline::new(
        pal.clone(),
        ComputePipelineCreateInfo {
            layouts: vec![layout.clone()],
            module: shader,
            work_group_size: (1, 1, 1),
            push_constants_size: None,
            debug_name: None,
        },
    )
    .unwrap();

    let mut command_buffer = pal.compute().command_buffer();
    command_buffer.compute_pass(&pipeline, None, |pass| {
        pass.bind_sets(0, vec![&set]);
        (1, 1, 1)
    });
    pal.compute().submit(None, command_buffer);

    Instant::now().duration_since(begin)
}

fn wgpu_test1(device: &wgpu::Device, queue: &wgpu::Queue, buffers: &[wgpu::Buffer]) -> Duration {
    let begin = Instant::now();

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::COMPUTE,
            binding: 0,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: Some(NonZeroU32::new(TEST1_BUFFER_COUNT as u32).unwrap()),
        }],
    });

    let mut bindings = Vec::with_capacity(TEST1_BUFFER_COUNT);
    for buffer in buffers {
        bindings.push(wgpu::BufferBinding {
            buffer: buffer,
            offset: 0,
            size: None,
        });
    }

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::BufferArray(&bindings),
        }],
    });

    const SHADER_BIN: &'static [u8] = include_bytes!("./shaders/test1_wgpu.comp.spv");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::util::make_spirv(&SHADER_BIN),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &shader,
        entry_point: "main",
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut compute_pass =
            encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
        compute_pass.set_pipeline(&pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(1, 1, 1);
    }
    queue.submit(Some(encoder.finish()));

    Instant::now().duration_since(begin)
}
