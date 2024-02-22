use ard_math::Vec2;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, types::*};
use half::f16;
use ordered_float::NotNan;

pub struct AmbientOcclusion {
    ctx: Context,
    /// AO map generation pipeline.
    gen: ComputePipeline,
    gen_layout: DescriptorSetLayout,
    /// AO map blurring pipeline.
    blur: ComputePipeline,
    blur_layout: DescriptorSetLayout,
    /// Sampling kernel UBO.
    kernel: Buffer,
    /// Noise texture.
    noise: Texture,
}

pub struct AoImage<const FIF: usize> {
    /// Unblurred AO image.
    image: Texture,
    /// Final blurred AO image.
    blurred: Texture,
    /// Sets for generating the AO image.
    gen_sets: [DescriptorSet; FIF],
    /// Sets for blurring the AO image.
    blur_sets: [DescriptorSet; FIF],
}

const AO_FORMAT: Format = Format::R16SFloat;

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

const AO_DEPTH_SAMPLER: Sampler = Sampler {
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

const AO_BLUR_SAMPLER: Sampler = Sampler {
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

const AO_NOISE_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
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
        // Sampling kernel
        let mut kernel = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of_val(&SSAO_KERNEL) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ssao_kernel_ubo".into()),
            },
        )
        .unwrap();

        kernel
            .write(0)
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(&SSAO_KERNEL));

        // Initialize noise texture
        let noise = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::R16SFloat,
                ty: TextureType::Type2D,
                width: 4,
                height: 4,
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
            bytemuck::cast_slice(&SSAO_NOISE),
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
                texture_extent: (4, 4, 1),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        ctx.main()
            .submit(Some("ssao_noise_upload"), commands)
            .wait_on(None);

        let shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./ao_construct.comp.spv")),
                debug_name: Some("ao_construct_shader".into()),
            },
        )
        .unwrap();

        let gen_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.ao_construct.clone(), layouts.camera.clone()],
                    module: shader,
                    work_group_size: (8, 8, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuAoConstructPushConstants>() as u32
                    ),
                    debug_name: Some("ao_construct_pipeline".into()),
                },
            )
            .unwrap();

        let shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./ao_blur.comp.spv")),
                debug_name: Some("ao_blur_shader".into()),
            },
        )
        .unwrap();

        let blur_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.ao_blur.clone()],
                    module: shader,
                    work_group_size: (8, 8, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuAoConstructPushConstants>() as u32
                    ),
                    debug_name: Some("ao_blur_pipeline".into()),
                },
            )
            .unwrap();

        Self {
            ctx: ctx.clone(),
            gen: gen_pipeline,
            gen_layout: layouts.ao_construct.clone(),
            blur: blur_pipeline,
            blur_layout: layouts.ao_blur.clone(),
            kernel,
            noise,
        }
    }

    pub fn generate<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        image: &'a AoImage<FIF>,
        camera: &'a CameraUbo,
    ) {
        let (width, height, _) = image.image.dims();
        let constants = [GpuAoConstructPushConstants {
            render_area: Vec2::new(width as f32, height as f32),
            inv_render_area: Vec2::new(1.0 / width as f32, 1.0 / height as f32),
            noise_scale: Vec2::new(width as f32, height as f32) / 4.0,
            radius: 0.5,
            bias: 0.025,
        }];

        commands.compute_pass(&self.gen, Some("ao_gen"), |pass| {
            pass.bind_sets(
                0,
                vec![&image.gen_sets[usize::from(frame)], camera.get_set(frame)],
            );
            pass.push_constants(bytemuck::cast_slice(&constants));

            let dispatch_x = (width as f32 / 8.0).ceil() as u32;
            let dispatch_y = (height as f32 / 8.0).ceil() as u32;
            ComputePassDispatch::Inline(dispatch_x, dispatch_y, 1)
        });

        commands.compute_pass(&self.blur, Some("ao_blur"), |pass| {
            pass.bind_sets(0, vec![&image.blur_sets[usize::from(frame)]]);
            pass.push_constants(bytemuck::cast_slice(&constants));
            ComputePassDispatch::Inline(width.div_ceil(8).max(1), height.div_ceil(8).max(1), 1)
        });
    }
}

impl<const FIF: usize> AoImage<FIF> {
    pub fn new(ao: &AmbientOcclusion, dims: (u32, u32)) -> Self {
        let image = Texture::new(
            ao.ctx.clone(),
            TextureCreateInfo {
                format: AO_FORMAT,
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
                debug_name: Some("ao_image".into()),
            },
        )
        .unwrap();

        let blurred = Texture::new(
            ao.ctx.clone(),
            TextureCreateInfo {
                format: AO_FORMAT,
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
                debug_name: Some("blurred_ao_image".into()),
            },
        )
        .unwrap();

        let gen_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.gen_layout.clone(),
                    debug_name: Some("ao_gen_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_CONSTRUCT_SET_AO_IMAGE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &image,
                        array_element: 0,
                        mip: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_CONSTRUCT_SET_NOISE_TEXTURE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &ao.noise,
                        array_element: 0,
                        sampler: AO_NOISE_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_CONSTRUCT_SET_KERNEL_SAMPLES_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &ao.kernel,
                        array_element: 0,
                    },
                },
            ]);

            set
        });

        let blur_sets = std::array::from_fn(|_| {
            let mut set = DescriptorSet::new(
                ao.ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: ao.blur_layout.clone(),
                    debug_name: Some("ao_blur_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: AO_BLUR_SET_INPUT_TEXTURE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::Texture {
                        texture: &image,
                        array_element: 0,
                        sampler: AO_BLUR_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                },
                DescriptorSetUpdate {
                    binding: AO_BLUR_SET_AO_IMAGE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageImage {
                        texture: &blurred,
                        array_element: 0,
                        mip: 0,
                    },
                },
            ]);

            set
        });

        Self {
            image,
            blurred,
            blur_sets,
            gen_sets,
        }
    }

    #[inline(always)]
    pub fn texture(&self) -> &Texture {
        &self.blurred
    }

    pub fn update_binding(&mut self, frame: Frame, src: &Texture) {
        self.gen_sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: AO_CONSTRUCT_SET_DEPTH_TEXTURE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: &src,
                array_element: 0,
                sampler: AO_DEPTH_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }
}

const SSAO_KERNEL: [f32; 64 * 4] = [
    0.0742, -0.0467, 0.0481, 0.0, -0.0207, 0.0736, 0.0648, 0.0, -0.0356, 0.0395, 0.0857, 0.0,
    0.0043, -0.0092, 0.1015, 0.0, 0.0467, 0.0743, 0.0549, 0.0, -0.0468, -0.0901, 0.0287, 0.0,
    0.0996, -0.0398, 0.0120, 0.0, -0.0417, 0.0748, 0.0702, 0.0, 0.0666, 0.0729, 0.0572, 0.0,
    -0.0892, 0.0544, 0.0544, 0.0, 0.0248, 0.0971, 0.0695, 0.0, -0.0064, 0.0523, 0.1151, 0.0,
    0.1015, 0.0822, 0.0167, 0.0, -0.0727, 0.1160, 0.0082, 0.0, -0.0073, 0.0868, 0.1135, 0.0,
    0.1161, -0.0936, 0.0090, 0.0, 0.1374, 0.0743, 0.0043, 0.0, 0.1374, 0.0882, 0.0081, 0.0, 0.0612,
    0.1155, 0.1106, 0.0, -0.0179, -0.0540, 0.1701, 0.0, 0.1208, -0.0272, 0.1413, 0.0, 0.0422,
    -0.0208, 0.1912, 0.0, -0.0095, 0.1987, 0.0547, 0.0, 0.2118, -0.0437, 0.0018, 0.0, 0.0986,
    0.1509, 0.1373, 0.0, -0.0944, -0.0683, 0.2068, 0.0, -0.0694, 0.1252, 0.2032, 0.0, -0.1180,
    0.1008, 0.2089, 0.0, -0.1797, -0.0640, 0.1943, 0.0, 0.0468, -0.1901, 0.2068, 0.0, 0.2666,
    0.0294, 0.1293, 0.0, -0.2450, 0.0747, 0.1767, 0.0, -0.2300, 0.0597, 0.2218, 0.0, -0.0167,
    -0.2511, 0.2275, 0.0, 0.2078, -0.2431, 0.1518, 0.0, 0.1991, -0.3109, 0.0011, 0.0, 0.2149,
    0.2077, 0.2424, 0.0, 0.1679, 0.3127, 0.1862, 0.0, 0.1849, -0.3539, 0.1212, 0.0, -0.2771,
    -0.0036, 0.3343, 0.0, 0.2979, -0.3393, 0.0059, 0.0, 0.4023, -0.2415, 0.0116, 0.0, -0.1897,
    0.3475, 0.2846, 0.0, 0.0118, 0.3149, 0.3963, 0.0, -0.4274, 0.0953, 0.2904, 0.0, -0.2734,
    -0.2861, 0.3747, 0.0, 0.2593, 0.3172, 0.3890, 0.0, -0.3387, -0.2983, 0.3728, 0.0, 0.5869,
    0.0637, 0.1379, 0.0, 0.2783, -0.4527, 0.3339, 0.0, -0.4370, 0.2580, 0.4051, 0.0, -0.6038,
    0.0138, 0.2936, 0.0, -0.5109, -0.0673, 0.4650, 0.0, 0.4341, 0.2735, 0.5012, 0.0, -0.4725,
    0.5177, 0.2395, 0.0, -0.2567, 0.0007, 0.7203, 0.0, -0.0380, -0.5292, 0.5841, 0.0, 0.1196,
    -0.6415, 0.4864, 0.0, 0.2373, 0.8036, 0.0457, 0.0, -0.0989, -0.5774, 0.6363, 0.0, -0.4696,
    -0.1857, 0.7341, 0.0, 0.8587, -0.2153, 0.2414, 0.0, 0.5738, 0.5794, 0.4768, 0.0, -0.3200,
    -0.2996, 0.8676, 0.0,
];

const SSAO_NOISE: [f16; 16 * 2] = [
    f16::from_f32_const(0.3191),
    f16::from_f32_const(0.4208),
    f16::from_f32_const(-0.2889),
    f16::from_f32_const(-0.4302),
    f16::from_f32_const(-0.8816),
    f16::from_f32_const(-0.1710),
    f16::from_f32_const(-0.2621),
    f16::from_f32_const(0.2785),
    f16::from_f32_const(0.5328),
    f16::from_f32_const(0.8616),
    f16::from_f32_const(-0.8078),
    f16::from_f32_const(0.9598),
    f16::from_f32_const(-0.2374),
    f16::from_f32_const(-0.4953),
    f16::from_f32_const(-0.4956),
    f16::from_f32_const(-0.5487),
    f16::from_f32_const(-0.1943),
    f16::from_f32_const(0.0714),
    f16::from_f32_const(0.7351),
    f16::from_f32_const(-0.5824),
    f16::from_f32_const(-0.0194),
    f16::from_f32_const(-0.2819),
    f16::from_f32_const(0.2566),
    f16::from_f32_const(-0.8426),
    f16::from_f32_const(-0.7865),
    f16::from_f32_const(0.2289),
    f16::from_f32_const(0.5914),
    f16::from_f32_const(-0.9091),
    f16::from_f32_const(-0.9770),
    f16::from_f32_const(0.5727),
    f16::from_f32_const(0.2358),
    f16::from_f32_const(0.1348),
];
