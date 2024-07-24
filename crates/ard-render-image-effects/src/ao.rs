use ard_ecs::resource::Resource;
use ard_math::{IVec2, Vec2};
use ard_pal::prelude::*;
use ard_render_base::{Frame, FRAMES_IN_FLIGHT};
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, types::*};
use ordered_float::NotNan;

#[derive(Copy, Clone, Resource)]
pub struct AoSettings {
    pub radius: f32,
    pub effect_falloff_range: f32,
    pub final_value_power: f32,
    pub denoise_blur_beta: f32,
    pub bilateral_filter_d: f32,
    pub bilateral_filter_r: f32,
    pub sample_distribution_power: f32,
    pub thin_occluder_compensation: f32,
    pub depth_mip_sampling_offset: f32,
}

impl Default for AoSettings {
    fn default() -> Self {
        Self {
            radius: 0.5,
            effect_falloff_range: 0.615,
            final_value_power: 2.7,
            denoise_blur_beta: 1.2,
            sample_distribution_power: 2.0,
            thin_occluder_compensation: 0.0,
            depth_mip_sampling_offset: 3.3,
            bilateral_filter_d: 9.0,
            bilateral_filter_r: 0.2,
        }
    }
}

pub struct AmbientOcclusion {
    ctx: Context,
    /// Depth prefiltering pipeline.
    depth_prefilter: ComputePipeline,
    depth_prefilter_layout: DescriptorSetLayout,
    /// AO main pass pipeline.
    main_pass: ComputePipeline,
    main_pass_layout: DescriptorSetLayout,
    /// AO denoising pipeline.
    denoise: ComputePipeline,
    denoise_layout: DescriptorSetLayout,
    // AO bilateral filter pipeline.
    filter: ComputePipeline,
    filter_layout: DescriptorSetLayout,
    /// Noise texture.
    noise: Texture,
}

pub struct AoImage {
    /// Prefiltered depths texture.
    _prefiltered_depth: Texture,
    /// Edge texture.
    _edges: Texture,
    /// AO image.
    image: Texture,
    /// Sets for depth prefiltering.
    depth_prefilter_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    /// Sets for the main AO pass.
    main_pass_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    /// Sets for the denoising pass.
    denoise_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    /// Sets for the bilateral filter pass.
    horz_filter_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    vert_filter_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

const WORK_GROUP_SIZE: u32 = 8;
const PREFILTERED_DEPTH_MIP_COUNT: usize = 5;

const AO_FORMAT: Format = Format::R16SFloat;
const PREFILTERED_DEPTH_FORMAT: Format = Format::R32SFloat;
const EDGES_FORMAT: Format = Format::R8Unorm;

const MAIN_PASS_DST: usize = 1;
const DENOISE_PASS_DST: usize = 0;
const HORZ_BLUR_PASS_DST: usize = 1;
const VERT_BLUR_PASS_DST: usize = 0;

pub const AO_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

const NEAREST_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

const LINEAR_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

const PREFILTERED_DEPTH_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

const NOISE_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::Repeat,
    address_v: SamplerAddressMode::Repeat,
    address_w: SamplerAddressMode::Repeat,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

impl AmbientOcclusion {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        // Initialize noise texture
        let noise = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::R8UInt,
                ty: TextureType::Type2D,
                width: 64,
                height: 64,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ssao_noise".into()),
            },
        )
        .unwrap();

        let noise_staging = Buffer::new_staging(
            ctx.clone(),
            QueueType::Main,
            Some(String::from("ssao_noise_staging")),
            include_bytes!("../bin/ao_noise.bin"),
        )
        .unwrap();

        let mut commands = ctx.main().command_buffer();
        commands.copy_buffer_to_texture(
            &noise,
            &noise_staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (64, 64, 1),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        ctx.main()
            .submit(Some("ssao_noise_upload"), commands)
            .wait_on(None);

        let depth_prefilter = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.ao_depth_prefilter.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(
                            env!("OUT_DIR"),
                            "./ao_depth_prefilter.comp.spv"
                        )),
                        debug_name: Some("ao_depth_prefilter_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                push_constants_size: Some(std::mem::size_of::<GpuGtaoPushConstants>() as u32),
                debug_name: Some("ao_depth_prefilter_pipeline".into()),
            },
        )
        .unwrap();

        let main_pass = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.ao_main_pass.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(env!("OUT_DIR"), "./ao_main_pass.comp.spv")),
                        debug_name: Some("ao_main_pass_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                push_constants_size: Some(std::mem::size_of::<GpuGtaoPushConstants>() as u32),
                debug_name: Some("ao_main_pass_pipeline".into()),
            },
        )
        .unwrap();

        let denoise = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.ao_denoise_pass.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(
                            env!("OUT_DIR"),
                            "./ao_denoise_pass.comp.spv"
                        )),
                        debug_name: Some("ao_denoise_pass_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                push_constants_size: Some(std::mem::size_of::<GpuGtaoPushConstants>() as u32),
                debug_name: Some("ao_denoise_pass_pipeline".into()),
            },
        )
        .unwrap();

        let filter = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.ao_bilateral_filter_pass.clone()],
                module: Shader::new(
                    ctx.clone(),
                    ShaderCreateInfo {
                        code: include_bytes!(concat!(
                            env!("OUT_DIR"),
                            "./ao_bilateral_filter.comp.spv"
                        )),
                        debug_name: Some("ao_bilateral_filter_pass_shader".into()),
                    },
                )
                .unwrap(),
                work_group_size: (WORK_GROUP_SIZE, WORK_GROUP_SIZE, 1),
                push_constants_size: Some(std::mem::size_of::<GpuGtaoPushConstants>() as u32),
                debug_name: Some("ao_bilateral_filter_pass_pipeline".into()),
            },
        )
        .unwrap();

        Self {
            ctx: ctx.clone(),
            depth_prefilter,
            depth_prefilter_layout: layouts.ao_depth_prefilter.clone(),
            main_pass,
            main_pass_layout: layouts.ao_main_pass.clone(),
            denoise,
            denoise_layout: layouts.ao_denoise_pass.clone(),
            filter,
            filter_layout: layouts.ao_bilateral_filter_pass.clone(),
            noise,
        }
    }

    pub fn generate<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        image: &'a AoImage,
        camera: &'a CameraUbo,
        settings: &AoSettings,
    ) {
        let (width, height, _) = image.image.dims();

        let camera_tan_half_fov = {
            let mut v = Vec2::ZERO;
            v.y = (camera.last().fov * 0.5).tan();
            v.x = v.y * (width as f32 / height as f32);
            v
        };

        let ndc_to_view_mul = Vec2::new(camera_tan_half_fov.x * 2.0, camera_tan_half_fov.y * -2.0);

        let ndc_to_view_add = Vec2::new(camera_tan_half_fov.x * -1.0, camera_tan_half_fov.y);

        let ndc_to_view_mul_x_pixel_size = Vec2::new(
            ndc_to_view_mul.x / width as f32,
            ndc_to_view_mul.y / height as f32,
        );

        let mut consts = [GpuGtaoPushConstants {
            viewport_size: IVec2::new(width as i32, height as i32),
            viewport_pixel_size: 1.0 / Vec2::new(width as f32, height as f32),
            camera_near_clip: camera.last().near,
            camera_tan_half_fov,
            ndc_to_view_mul,
            ndc_to_view_add,
            ndc_to_view_mul_x_pixel_size,
            effect_radius: settings.radius,
            effect_falloff_range: settings.effect_falloff_range,
            radius_multiplier: 1.457,
            final_value_power: settings.final_value_power,
            denoise_blur_beta: settings.denoise_blur_beta,
            bilateral_filter_d: settings.bilateral_filter_d,
            bilateral_filter_r: settings.bilateral_filter_r,
            sample_distribution_power: settings.sample_distribution_power,
            thin_occluder_compensation: settings.thin_occluder_compensation,
            depth_mip_sampling_offset: settings.depth_mip_sampling_offset,
            blur_dir: IVec2::new(1, 0),
        }];

        commands.compute_pass(
            &self.depth_prefilter,
            Some("ao_depth_prefiltering"),
            |pass| {
                pass.bind_sets(0, vec![&image.depth_prefilter_sets[usize::from(frame)]]);
                pass.push_constants(bytemuck::cast_slice(&consts));

                let dispatch_x = width.div_ceil(WORK_GROUP_SIZE * 2);
                let dispatch_y = height.div_ceil(WORK_GROUP_SIZE * 2);
                ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
            },
        );

        commands.compute_pass(&self.main_pass, Some("ao_main_pass"), |pass| {
            pass.bind_sets(0, vec![&image.main_pass_sets[usize::from(frame)]]);
            pass.push_constants(bytemuck::cast_slice(&consts));

            let dispatch_x = width.div_ceil(WORK_GROUP_SIZE);
            let dispatch_y = height.div_ceil(WORK_GROUP_SIZE);
            ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
        });

        commands.compute_pass(&self.denoise, Some("ao_denoise"), |pass| {
            pass.bind_sets(0, vec![&image.denoise_sets[usize::from(frame)]]);
            pass.push_constants(bytemuck::cast_slice(&consts));

            let dispatch_x = width.div_ceil(WORK_GROUP_SIZE * 2);
            let dispatch_y = height.div_ceil(WORK_GROUP_SIZE);
            ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
        });

        commands.compute_pass(&self.filter, Some("ao_horz_bilateral_filter"), |pass| {
            pass.bind_sets(0, vec![&image.horz_filter_sets[usize::from(frame)]]);
            pass.push_constants(bytemuck::cast_slice(&consts));

            let dispatch_x = width.div_ceil(WORK_GROUP_SIZE);
            let dispatch_y = height.div_ceil(WORK_GROUP_SIZE);
            ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
        });

        consts[0].blur_dir = IVec2::new(0, 1);

        commands.compute_pass(&self.filter, Some("ao_vert_bilateral_filter"), |pass| {
            pass.bind_sets(0, vec![&image.vert_filter_sets[usize::from(frame)]]);
            pass.push_constants(bytemuck::cast_slice(&consts));

            let dispatch_x = width.div_ceil(WORK_GROUP_SIZE);
            let dispatch_y = height.div_ceil(WORK_GROUP_SIZE);
            ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
        });
    }
}

impl AoImage {
    pub fn new(ao: &AmbientOcclusion, dims: (u32, u32)) -> Self {
        let prefiltered_depth = Texture::new(
            ao.ctx.clone(),
            TextureCreateInfo {
                format: PREFILTERED_DEPTH_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: PREFILTERED_DEPTH_MIP_COUNT,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ao_prefiltered_depth_image".into()),
            },
        )
        .unwrap();

        let edges = Texture::new(
            ao.ctx.clone(),
            TextureCreateInfo {
                format: EDGES_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ao_edges".into()),
            },
        )
        .unwrap();

        let image = Texture::new(
            ao.ctx.clone(),
            TextureCreateInfo {
                format: AO_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 2,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ao_image".into()),
            },
        )
        .unwrap();

        let depth_prefilter_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.depth_prefilter_layout.clone(),
                    debug_name: Some("ao_depth_prefilter_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_DEPTH_PREFILTER_SET_OUT_DEPTH_0_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DEPTH_PREFILTER_SET_OUT_DEPTH_1_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        mip: 1,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DEPTH_PREFILTER_SET_OUT_DEPTH_2_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        mip: 2,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DEPTH_PREFILTER_SET_OUT_DEPTH_3_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        mip: 3,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DEPTH_PREFILTER_SET_OUT_DEPTH_4_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        mip: 4,
                    },
                },
            ]);

            set
        });

        let main_pass_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.main_pass_layout.clone(),
                    debug_name: Some("ao_main_pass_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_MAIN_PASS_SET_SRC_DEPTH_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &prefiltered_depth,
                        array_element: 0,
                        base_mip: 0,
                        mip_count: prefiltered_depth.mip_count(),
                        sampler: PREFILTERED_DEPTH_SAMPLER,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_MAIN_PASS_SET_NOISE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &ao.noise,
                        array_element: 0,
                        base_mip: 0,
                        mip_count: 1,
                        sampler: NOISE_SAMPLER,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_MAIN_PASS_SET_EDGES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &edges,
                        array_element: 0,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_MAIN_PASS_SET_WORKING_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: MAIN_PASS_DST,
                        mip: 0,
                    },
                },
            ]);

            set
        });

        let denoise_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.denoise_layout.clone(),
                    debug_name: Some("ao_main_pass_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_DENOISE_PASS_SET_SRC_DEPTH_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &image,
                        array_element: MAIN_PASS_DST,
                        base_mip: 0,
                        mip_count: 1,
                        sampler: LINEAR_SAMPLER,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DENOISE_PASS_SET_EDGES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &edges,
                        array_element: 0,
                        base_mip: 0,
                        mip_count: 1,
                        sampler: LINEAR_SAMPLER,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_DENOISE_PASS_SET_OUTPUT_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: DENOISE_PASS_DST,
                        mip: 0,
                    },
                },
            ]);

            set
        });

        let horz_filter_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.filter_layout.clone(),
                    debug_name: Some("ao_horz_bilateral_filter_pass_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_BILATERAL_FILTER_PASS_SET_SOURCE_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: DENOISE_PASS_DST,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_BILATERAL_FILTER_PASS_SET_OUTPUT_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: HORZ_BLUR_PASS_DST,
                        mip: 0,
                    },
                },
            ]);

            set
        });

        let vert_filter_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.filter_layout.clone(),
                    debug_name: Some("ao_vert_bilateral_filter_pass_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_BILATERAL_FILTER_PASS_SET_SOURCE_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: HORZ_BLUR_PASS_DST,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_BILATERAL_FILTER_PASS_SET_OUTPUT_AO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: VERT_BLUR_PASS_DST,
                        mip: 0,
                    },
                },
            ]);

            set
        });

        Self {
            _prefiltered_depth: prefiltered_depth,
            image,
            _edges: edges,
            depth_prefilter_sets,
            main_pass_sets,
            denoise_sets,
            horz_filter_sets,
            vert_filter_sets,
        }
    }

    #[inline(always)]
    pub fn texture(&self) -> &Texture {
        &self.image
    }

    pub fn update_binding(&mut self, frame: Frame, src_depth: &Texture) {
        self.depth_prefilter_sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: AO_DEPTH_PREFILTER_SET_SRC_DEPTH_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src_depth,
                array_element: 0,
                sampler: NEAREST_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }
}
