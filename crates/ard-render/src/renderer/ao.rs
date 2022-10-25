use ard_math::Vec2;
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use half::f16;
use ordered_float::NotNan;

use crate::shader_constants::FRAMES_IN_FLIGHT;

const AO_SAMPLE_TEX_BINDING: u32 = 0;
const AO_OUTPUT_IMG_BINDING: u32 = 1;
const AO_CAMERA_BINDING: u32 = 2;
const AO_NOISE_BINDING: u32 = 3;
const AO_KERNAL_BINDING: u32 = 4;

pub(crate) const AO_SAMPLER: Sampler = Sampler {
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

pub(crate) struct AmbientOcclusion {
    ctx: Context,
    layout: DescriptorSetLayout,
    gen_pipeline: ComputePipeline,
    blur_pipeline: ComputePipeline,
    /// Sampling kernel UBO.
    kernel: Buffer,
    /// Noise texture.
    noise: Texture,
    /// Default AO texture.
    default_ao: Texture,
}

pub(crate) struct AoImage {
    ctx: Context,
    image: Texture,
    blurred: Texture,
    /// Sets for generating the AO image.
    gen_sets: Vec<DescriptorSet>,
    /// Sets for blurring the generated image.
    blur_sets: Vec<DescriptorSet>,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct AoConstructPushConstants {
    screen_size: Vec2,
    inv_screen_size: Vec2,
    noise_scale: Vec2,
    radius: f32,
    bias: f32,
}

unsafe impl Pod for AoConstructPushConstants {}
unsafe impl Zeroable for AoConstructPushConstants {}

impl AmbientOcclusion {
    pub fn new(ctx: &Context) -> Self {
        // Initialize sampling kernel
        let kernel = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of_val(&SSAO_KERNEL) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("ssao_kernel_ubo")),
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
                format: TextureFormat::Rg16SFloat,
                ty: TextureType::Type2D,
                width: 4,
                height: 4,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("ssao_noise")),
            },
        )
        .unwrap();

        // Default AO texture.
        let default_ao = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::R16SFloat,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("default_ao")),
            },
        )
        .unwrap();

        let noise_staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("ssao_noise_staging")),
            bytemuck::cast_slice(&SSAO_NOISE),
        )
        .unwrap();

        let ao_staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("default_ao_staging")),
            bytemuck::cast_slice(&[f16::ONE]),
        )
        .unwrap();

        let mut commands = ctx.transfer().command_buffer();
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
        for frame in 0..FRAMES_IN_FLIGHT {
            commands.copy_buffer_to_texture(
                &default_ao,
                &ao_staging,
                BufferTextureCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    buffer_array_element: 0,
                    texture_offset: (0, 0, 0),
                    texture_extent: (1, 1, 1),
                    texture_mip_level: 0,
                    texture_array_element: frame,
                },
            );
        }
        ctx.transfer().submit(Some("ssao_noise_upload"), commands);

        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    DescriptorBinding {
                        binding: AO_SAMPLE_TEX_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: AO_OUTPUT_IMG_BINDING,
                        ty: DescriptorType::StorageImage(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: AO_CAMERA_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: AO_NOISE_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: AO_KERNAL_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/ao_construct.comp.spv"),
                debug_name: Some(String::from("ao_construct_shader")),
            },
        )
        .unwrap();

        let gen_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layout.clone()],
                module: shader,
                work_group_size: (8, 8, 1),
                push_constants_size: Some(std::mem::size_of::<AoConstructPushConstants>() as u32),
                debug_name: Some(String::from("ao_construct_pipeline")),
            },
        )
        .unwrap();

        let shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/ao_blur.comp.spv"),
                debug_name: Some(String::from("ao_blur_shader")),
            },
        )
        .unwrap();

        let blur_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layout.clone()],
                module: shader,
                work_group_size: (4, 4, 1),
                push_constants_size: Some(std::mem::size_of::<AoConstructPushConstants>() as u32),
                debug_name: Some(String::from("ao_blur_pipeline")),
            },
        )
        .unwrap();

        Self {
            ctx: ctx.clone(),
            layout,
            gen_pipeline,
            blur_pipeline,
            kernel,
            noise,
            default_ao,
        }
    }

    #[inline(always)]
    pub fn default_texture(&self) -> &Texture {
        &self.default_ao
    }

    #[inline(always)]
    pub fn layout(&self) -> &DescriptorSetLayout {
        &self.layout
    }
}

impl AoImage {
    pub fn new(ctx: &Context, layout: &DescriptorSetLayout) -> Self {
        let image = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::R16SFloat,
                ty: TextureType::Type2D,
                width: 128,
                height: 128,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("ao_image")),
            },
        )
        .unwrap();

        let blurred = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::R16SFloat,
                ty: TextureType::Type2D,
                width: 128,
                height: 128,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("blurred_ao_image")),
            },
        )
        .unwrap();

        let mut gen_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut blur_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            gen_sets.push(
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layout.clone(),
                        debug_name: Some(format!("ao_image_gen_set_{frame}")),
                    },
                )
                .unwrap(),
            );

            blur_sets.push(
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layout.clone(),
                        debug_name: Some(format!("ao_image_blur_set_{frame}")),
                    },
                )
                .unwrap(),
            );
        }

        Self {
            ctx: ctx.clone(),
            image,
            blurred,
            gen_sets,
            blur_sets,
        }
    }

    #[inline(always)]
    pub fn texture(&self) -> &Texture {
        &self.blurred
    }

    pub fn resize_to_fit(&mut self, width: u32, height: u32) {
        let (old_width, old_height, _) = self.image.dims();
        if old_width != width || old_height != height {
            self.image = Texture::new(
                self.ctx.clone(),
                TextureCreateInfo {
                    format: TextureFormat::R16SFloat,
                    ty: TextureType::Type2D,
                    width,
                    height,
                    depth: 1,
                    array_elements: FRAMES_IN_FLIGHT,
                    mip_levels: 1,
                    texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some(String::from("ao_image")),
                },
            )
            .unwrap();

            self.blurred = Texture::new(
                self.ctx.clone(),
                TextureCreateInfo {
                    format: TextureFormat::R16SFloat,
                    ty: TextureType::Type2D,
                    width,
                    height,
                    depth: 1,
                    array_elements: FRAMES_IN_FLIGHT,
                    mip_levels: 1,
                    texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some(String::from("blurred_ao_image")),
                },
            )
            .unwrap();
        }
    }

    #[inline]
    pub fn update_set(
        &mut self,
        frame: usize,
        ao: &AmbientOcclusion,
        camera_ubo: &Buffer,
        depth_src: &Texture,
    ) {
        self.gen_sets[frame].update(&[
            DescriptorSetUpdate {
                binding: AO_CAMERA_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: camera_ubo,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: AO_OUTPUT_IMG_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.image,
                    array_element: frame,
                    mip: 0,
                },
            },
            DescriptorSetUpdate {
                binding: AO_SAMPLE_TEX_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: depth_src,
                    array_element: frame,
                    sampler: AO_DEPTH_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: AO_KERNAL_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: &ao.kernel,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: AO_NOISE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &ao.noise,
                    array_element: 0,
                    sampler: AO_NOISE_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);

        self.blur_sets[frame].update(&[
            DescriptorSetUpdate {
                binding: AO_OUTPUT_IMG_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.blurred,
                    array_element: frame,
                    mip: 0,
                },
            },
            DescriptorSetUpdate {
                binding: AO_SAMPLE_TEX_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &self.image,
                    array_element: frame,
                    sampler: AO_BLUR_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);
    }

    pub fn generate<'a, 'b>(
        &'a self,
        frame: usize,
        ao: &AmbientOcclusion,
        commands: &'b mut CommandBuffer<'a>,
    ) {
        commands.compute_pass(|pass| {
            let (width, height, _) = self.image.dims();
            let constants = [AoConstructPushConstants {
                screen_size: Vec2::new(width as f32, height as f32),
                inv_screen_size: Vec2::new(1.0 / width as f32, 1.0 / height as f32),
                noise_scale: Vec2::new(width as f32, height as f32) / 4.0,
                radius: 0.3,
                bias: 0.025,
            }];

            // Generate the image
            pass.bind_pipeline(ao.gen_pipeline.clone());
            pass.bind_sets(0, vec![&self.gen_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&constants));

            let dispatch_x = (width as f32 / 8.0).ceil() as u32;
            let dispatch_y = (height as f32 / 8.0).ceil() as u32;
            pass.dispatch(dispatch_x, dispatch_y, 1);

            // Blur the image
            pass.bind_pipeline(ao.blur_pipeline.clone());
            pass.bind_sets(0, vec![&self.blur_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&constants));
            pass.dispatch(width, height, 1);
        });
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
