use std::ops::Div;

use ard_math::IVec2;
use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, consts::*, types::*};
use ordered_float::NotNan;

pub struct SunShafts {
    texture: Texture,
    pipeline: ComputePipeline,
    sets: Vec<DescriptorSet>,
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

const BLOCK_SIZE: u32 = 16;

impl SunShafts {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        frames_in_flight: usize,
        dims: (u32, u32),
    ) -> Self {
        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./sun_shafts.comp.spv")),
                debug_name: Some("sun_shafts_shader".into()),
            },
        )
        .unwrap();

        let pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![
                    layouts.sun_shafts.clone(),
                    layouts.camera.clone(),
                    layouts.global.clone(),
                ],
                module,
                work_group_size: (BLOCK_SIZE, BLOCK_SIZE, 1),
                push_constants_size: Some(std::mem::size_of::<GpuSunShaftsPushConstants>() as u32),
                debug_name: Some("sun_shafts_pipeline".into()),
            },
        )
        .unwrap();

        let sets = (0..frames_in_flight)
            .into_iter()
            .map(|frame| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.sun_shafts.clone(),
                        debug_name: Some(format!("sun_shafts_{frame}")),
                    },
                )
                .unwrap()
            })
            .collect();

        Self {
            texture: Self::create_texture(ctx, dims),
            pipeline,
            sets,
        }
    }

    #[inline(always)]
    pub fn image(&self) -> &Texture {
        &self.texture
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.texture = Self::create_texture(ctx, dims);
    }

    pub fn update_binds(
        &mut self,
        frame: Frame,
        global_lighting_info: &Buffer,
        sun_shadow_info: &Buffer,
        cascades: [&Texture; MAX_SHADOW_CASCADES],
        depth: &Texture,
    ) {
        self.sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: SUN_SHAFTS_SET_OUTPUT_TEX_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.texture,
                    array_element: 0,
                    mip: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SUN_SHAFTS_SET_SOURCE_DEPTH_BINDING,
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
                binding: SUN_SHAFTS_SET_GLOBAL_LIGHTING_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: global_lighting_info,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: SUN_SHAFTS_SET_SUN_SHADOW_INFO_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: sun_shadow_info,
                    array_element: 0,
                },
            },
        ]);

        let updates = std::array::from_fn::<_, MAX_SHADOW_CASCADES, _>(|i| DescriptorSetUpdate {
            binding: SUN_SHAFTS_SET_SHADOW_CASCADES_BINDING,
            array_element: i,
            value: DescriptorValue::Texture {
                texture: cascades[i],
                array_element: 0,
                sampler: SHADOW_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        });

        self.sets[usize::from(frame)].update(&updates);
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
        global_set: &'a DescriptorSet,
    ) {
        commands.compute_pass(&self.pipeline, Some("sun_shafts"), |pass| {
            let (width, height, _) = self.texture.dims();
            let params = [GpuSunShaftsPushConstants {
                output_dims: IVec2::new(width as i32, height as i32),
            }];

            pass.bind_sets(
                0,
                vec![
                    &self.sets[usize::from(frame)],
                    camera.get_set(frame),
                    global_set,
                ],
            );
            pass.push_constants(bytemuck::cast_slice(&params));

            let (width, height, _) = self.texture.dims();
            ComputePassDispatch::Inline(width.div_ceil(BLOCK_SIZE), height.div_ceil(BLOCK_SIZE), 1)
        });
    }

    fn create_texture(ctx: &Context, dims: (u32, u32)) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba16SFloat,
                ty: TextureType::Type2D,
                width: dims.0.div(2).max(1),
                height: dims.1.div(2).max(1),
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sun_shafts".into()),
            },
        )
        .unwrap()
    }
}
