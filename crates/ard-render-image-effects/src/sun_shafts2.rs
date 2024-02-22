use ard_math::IVec2;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, consts::*, types::*};
use ordered_float::NotNan;

const WORK_GROUP_SIZE: u32 = 16;
const SAMPLE_WORK_GROUP_SIZE: u32 = 128;

pub struct SunShafts {
    line_count: usize,
    sample_count: usize,
    initial_sample_count: usize,
    epipolar_lines: Buffer,
    epipolar_samples: Buffer,
    sample_dispatch_buffer: Buffer,
    sun_shafts_texture: Texture,
    line_setup_pipeline: ComputePipeline,
    line_setup_sets: Vec<DescriptorSet>,
    refine_pipeline: ComputePipeline,
    refine_sets: Vec<DescriptorSet>,
    sample_pipeline: ComputePipeline,
    sample_sets: Vec<DescriptorSet>,
    interpolation_pipeline: ComputePipeline,
    interpolation_sets: Vec<DescriptorSet>,
}

const DEPTH_SRC_IMAGE_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: None,
    unnormalize_coords: false,
};

const SHADOW_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToBorder,
    address_v: SamplerAddressMode::ClampToBorder,
    address_w: SamplerAddressMode::ClampToBorder,
    anisotropy: None,
    compare: Some(CompareOp::Less),
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: Some(BorderColor::FloatOpaqueWhite),
    unnormalize_coords: false,
};

impl SunShafts {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        frames_in_flight: usize,
        dims: (u32, u32),
    ) -> Self {
        let line_count = Self::line_count_from_dims(dims);
        let sample_count = Self::sample_count_from_dims(dims);
        let initial_sample_count = 64;

        let epipolar_lines = Self::create_line_buffer(ctx, line_count, sample_count);
        let epipolar_samples = Self::create_sample_buffer(ctx, line_count, sample_count);
        let sample_dispatch_buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<DispatchIndirect>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sun_shaft_indirect_dispatch".into()),
            },
        )
        .unwrap();
        let sun_shafts_texture = Self::create_output_texture(ctx, dims);

        let line_setup_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.sun_shaft_line_setup.clone(), layouts.camera.clone()],
                    module: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./shafts_gen_lines.comp.spv"
                            )),
                            debug_name: Some("sun_shaft_line_setup_shader".into()),
                        },
                    )
                    .unwrap(),
                    work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuSunShaftGenPushConstants>() as u32
                    ),
                    debug_name: Some("sun_shaft_line_setup_pipeline".into()),
                },
            )
            .unwrap();

        let refine_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.sun_shaft_refine.clone(), layouts.camera.clone()],
                    module: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./shafts_refine.comp.spv"
                            )),
                            debug_name: Some("sun_shaft_refine_shader".into()),
                        },
                    )
                    .unwrap(),
                    work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuSunShaftGenPushConstants>() as u32
                    ),
                    debug_name: Some("sun_shaft_refine_pipeline".into()),
                },
            )
            .unwrap();

        let sample_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.sun_shaft_sample.clone(), layouts.camera.clone()],
                    module: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./shafts_sample.comp.spv"
                            )),
                            debug_name: Some("sun_shaft_sample_shader".into()),
                        },
                    )
                    .unwrap(),
                    work_group_size: (SAMPLE_WORK_GROUP_SIZE, 1, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuSunShaftGenPushConstants>() as u32
                    ),
                    debug_name: Some("sun_shaft_sample_pipeline".into()),
                },
            )
            .unwrap();

        let interpolation_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![
                        layouts.sun_shaft_interpolation.clone(),
                        layouts.camera.clone(),
                    ],
                    module: Shader::new(
                        ctx.clone(),
                        ShaderCreateInfo {
                            code: include_bytes!(concat!(
                                env!("OUT_DIR"),
                                "./shafts_interpolate.comp.spv"
                            )),
                            debug_name: Some("sun_shaft_interpolation_shader".into()),
                        },
                    )
                    .unwrap(),
                    work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuSunShaftGenPushConstants>() as u32
                    ),
                    debug_name: Some("sun_shaft_interpolation_pipeline".into()),
                },
            )
            .unwrap();

        let line_setup_sets = (0..frames_in_flight)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.sun_shaft_line_setup.clone(),
                        debug_name: Some(format!("sun_shaft_line_setup_set_{i}")),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_LINE_SETUP_SET_EPIPOLAR_LINES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_lines,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_LINE_SETUP_SET_EPIPOLAR_SAMPLES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_samples,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_LINE_SETUP_SET_SUN_SHAFT_INDIRECT_DISPATCH_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &sample_dispatch_buffer,
                            array_element: 0,
                        },
                    },
                ]);

                set
            })
            .collect();

        let refine_sets = (0..frames_in_flight)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.sun_shaft_refine.clone(),
                        debug_name: Some(format!("sun_shaft_refine_set_{i}")),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_REFINE_SET_EPIPOLAR_LINES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_lines,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_REFINE_SET_EPIPOLAR_SAMPLES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_samples,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_REFINE_SET_SUN_SHAFT_INDIRECT_DISPATCH_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &sample_dispatch_buffer,
                            array_element: 0,
                        },
                    },
                ]);

                set
            })
            .collect();

        let sample_sets = (0..frames_in_flight)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.sun_shaft_sample.clone(),
                        debug_name: Some(format!("sun_shaft_sample_set_{i}")),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_SAMPLE_SET_EPIPOLAR_LINES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_lines,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_SAMPLE_SET_EPIPOLAR_SAMPLES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_samples,
                            array_element: 0,
                        },
                    },
                ]);

                set
            })
            .collect();

        let interpolation_sets = (0..frames_in_flight)
            .map(|i| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.sun_shaft_interpolation.clone(),
                        debug_name: Some(format!("sun_shaft_interpolation_set_{i}")),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_INTERPOLATION_SET_EPIPOLAR_LINES_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &epipolar_lines,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: SUN_SHAFT_INTERPOLATION_SET_OUTPUT_TEX_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageImage {
                            texture: &sun_shafts_texture,
                            array_element: 0,
                            mip: 0,
                        },
                    },
                ]);

                set
            })
            .collect();

        Self {
            line_count,
            sample_count,
            initial_sample_count,
            epipolar_lines,
            epipolar_samples,
            sample_dispatch_buffer,
            sun_shafts_texture,
            line_setup_pipeline,
            line_setup_sets,
            refine_pipeline,
            refine_sets,
            sample_pipeline,
            sample_sets,
            interpolation_pipeline,
            interpolation_sets,
        }
    }

    #[inline(always)]
    pub fn image(&self) -> &Texture {
        &self.sun_shafts_texture
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.line_count = Self::line_count_from_dims(dims);
        self.sample_count = Self::sample_count_from_dims(dims);

        self.epipolar_lines = Self::create_line_buffer(ctx, self.line_count, self.sample_count);
        self.epipolar_samples = Self::create_sample_buffer(ctx, self.line_count, self.sample_count);
        self.sun_shafts_texture = Self::create_output_texture(ctx, dims);

        self.line_setup_sets.iter_mut().for_each(|set| {
            set.update(&[
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_LINE_SETUP_SET_EPIPOLAR_LINES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_lines,
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_LINE_SETUP_SET_EPIPOLAR_SAMPLES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_samples,
                        array_element: 0,
                    },
                },
            ]);
        });

        self.refine_sets.iter_mut().for_each(|set| {
            set.update(&[
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_REFINE_SET_EPIPOLAR_LINES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_lines,
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_REFINE_SET_EPIPOLAR_SAMPLES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_samples,
                        array_element: 0,
                    },
                },
            ]);
        });

        self.sample_sets.iter_mut().for_each(|set| {
            set.update(&[
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_SAMPLE_SET_EPIPOLAR_LINES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_lines,
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_SAMPLE_SET_EPIPOLAR_SAMPLES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_samples,
                        array_element: 0,
                    },
                },
            ]);
        });

        self.interpolation_sets.iter_mut().for_each(|set| {
            set.update(&[
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_INTERPOLATION_SET_OUTPUT_TEX_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &self.sun_shafts_texture,
                        array_element: 0,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: SUN_SHAFT_INTERPOLATION_SET_EPIPOLAR_LINES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &self.epipolar_lines,
                        array_element: 0,
                    },
                },
            ]);
        });
    }

    pub fn update_binds(
        &mut self,
        frame: Frame,
        global_lighting: &Buffer,
        sun_shadow_info: &Buffer,
        shadow_cascades: [&Texture; MAX_SHADOW_CASCADES],
        depth: &Texture,
    ) {
        self.line_setup_sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: SUN_SHAFT_LINE_SETUP_SET_SOURCE_DEPTH_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: depth,
                    array_element: 0,
                    sampler: DEPTH_SRC_IMAGE_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: SUN_SHAFT_LINE_SETUP_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: global_lighting,
                    array_element: 0,
                },
            },
        ]);

        self.sample_sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: SUN_SHAFT_SAMPLE_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: global_lighting,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SUN_SHAFT_SAMPLE_SET_SUN_SHADOW_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: sun_shadow_info,
                    array_element: 0,
                },
            },
        ]);

        let updates = std::array::from_fn::<_, MAX_SHADOW_CASCADES, _>(|i| DescriptorSetUpdate {
            binding: SUN_SHAFT_SAMPLE_SET_SHADOW_CASCADES_BINDING,
            array_element: i,
            value: DescriptorValue::Texture {
                texture: shadow_cascades[i],
                array_element: 0,
                sampler: SHADOW_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        });
        self.sample_sets[usize::from(frame)].update(&updates);

        self.interpolation_sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: SUN_SHAFT_INTERPOLATION_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: global_lighting,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SUN_SHAFT_INTERPOLATION_SET_SOURCE_DEPTH_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: depth,
                    array_element: 0,
                    sampler: DEPTH_SRC_IMAGE_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);
    }

    pub fn transfer_image_ownership<'a>(
        &'a self,
        commands: &mut CommandBuffer<'a>,
        new_queue: QueueType,
    ) {
        commands.transfer_texture_ownership(&self.sun_shafts_texture, 0, 0, 1, new_queue, None);
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
    ) {
        let (width, height, _) = self.sun_shafts_texture.dims();
        let params = [GpuSunShaftGenPushConstants {
            line_count: self.line_count as u32,
            sample_count_per_line: self.sample_count as u32,
            initial_sample_count: self.initial_sample_count as u32,
            samples_per_work_group: SAMPLE_WORK_GROUP_SIZE,
            low_sample_minimum: 10,
            steps_per_sample: 100,
            depth_threshold: 0.5,
            output_dims: IVec2::new(width as i32, height as i32),
        }];

        commands.compute_pass(
            &self.line_setup_pipeline,
            Some("sun_shaft_line_setup"),
            |pass| {
                pass.bind_sets(
                    0,
                    vec![
                        &self.line_setup_sets[usize::from(frame)],
                        camera.get_set(frame),
                    ],
                );
                pass.push_constants(bytemuck::cast_slice(&params));

                ComputePassDispatch::Inline(
                    (self.sample_count as u32).div_ceil(WORK_GROUP_SIZE),
                    (self.line_count as u32).div_ceil(WORK_GROUP_SIZE),
                    1,
                )
            },
        );

        commands.compute_pass(&self.refine_pipeline, Some("sun_shaft_refine"), |pass| {
            pass.bind_sets(
                0,
                vec![&self.refine_sets[usize::from(frame)], camera.get_set(frame)],
            );
            pass.push_constants(bytemuck::cast_slice(&params));

            ComputePassDispatch::Inline(
                (self.sample_count as u32).div_ceil(WORK_GROUP_SIZE),
                (self.line_count as u32).div_ceil(WORK_GROUP_SIZE),
                1,
            )
        });

        commands.compute_pass(&self.sample_pipeline, Some("sun_shaft_sampling"), |pass| {
            pass.bind_sets(
                0,
                vec![&self.sample_sets[usize::from(frame)], camera.get_set(frame)],
            );
            pass.push_constants(bytemuck::cast_slice(&params));

            ComputePassDispatch::Indirect {
                buffer: &self.sample_dispatch_buffer,
                array_element: 0,
                offset: 0,
            }
        });

        commands.compute_pass(
            &self.interpolation_pipeline,
            Some("sun_shaft_interpolation"),
            |pass| {
                pass.bind_sets(
                    0,
                    vec![
                        &self.interpolation_sets[usize::from(frame)],
                        camera.get_set(frame),
                    ],
                );
                pass.push_constants(bytemuck::cast_slice(&params));

                ComputePassDispatch::Inline(
                    (width as u32).div_ceil(WORK_GROUP_SIZE),
                    (height as u32).div_ceil(WORK_GROUP_SIZE),
                    1,
                )
            },
        );
    }

    #[inline(always)]
    fn line_count_from_dims(_dims: (u32, u32)) -> usize {
        // dims.0.min(dims.1).max(512) as usize
        1024
    }

    #[inline(always)]
    fn sample_count_from_dims(dims: (u32, u32)) -> usize {
        dims.0.max(dims.1).div_ceil(4).max(400) as usize
    }

    fn create_line_buffer(ctx: &Context, line_count: usize, sample_count: usize) -> Buffer {
        Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (line_count * sample_count * std::mem::size_of::<GpuSunShaftSample>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sun_shaft_epipolar_lines".into()),
            },
        )
        .unwrap()
    }

    fn create_sample_buffer(ctx: &Context, line_count: usize, sample_count: usize) -> Buffer {
        Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: ((1 + (line_count * sample_count)) * std::mem::size_of::<u32>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sun_shaft_epipolar_sample_indices".into()),
            },
        )
        .unwrap()
    }

    fn create_output_texture(ctx: &Context, dims: (u32, u32)) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba16SFloat,
                ty: TextureType::Type2D,
                width: dims.0.div_ceil(2).max(1),
                height: dims.1.div_ceil(2).max(1),
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sun_shafts".into()),
            },
        )
        .unwrap()
    }
}
