use std::time::Duration;

use ard_ecs::resource::Resource;
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, FRAMES_IN_FLIGHT};
use ard_render_camera::ubo::CameraUbo;
use ard_render_si::{bindings::*, consts::*, types::*};
use ordered_float::NotNan;

use crate::bloom::BLOOM_SAMPLE_FILTER;

const HISTOGRAM_GEN_BLOCK_SIZE: u32 = 16;

const HISTOGRAM_SRC_IMAGE_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: None,
    unnormalize_coords: true,
};

const TONEMAPPING_SRC_IMAGE_SAMPLER: Sampler = Sampler {
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

#[derive(Copy, Clone, Resource)]
pub struct TonemappingSettings {
    pub min_luminance: f32,
    pub max_luminance: f32,
    pub gamma: f32,
    pub exposure: f32,
    pub auto_exposure_rate: f32,
}

impl Default for TonemappingSettings {
    fn default() -> Self {
        Self {
            min_luminance: -1.3,
            max_luminance: 3.0,
            gamma: 2.2,
            exposure: 0.5,
            auto_exposure_rate: 4.0,
        }
    }
}

pub struct Tonemapping {
    _histogram: Buffer,
    _luminance: Buffer,
    screen_size: (u32, u32),
    /// Generates the luminance histogram.
    histogram_gen_pipeline: ComputePipeline,
    histogram_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    /// Computes luminance from the histogram.
    luminance_comp_pipeline: ComputePipeline,
    luminance_set: DescriptorSet,
    /// Tonemaps using adaptive luminance.
    tonemapping_pipeline: GraphicsPipeline,
    tonemapping_sets: [DescriptorSet; FRAMES_IN_FLIGHT],
}

impl Tonemapping {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let histogram = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (ADAPTIVE_LUM_HISTOGRAM_SIZE * std::mem::size_of::<u32>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("luminance_histogram".into()),
            },
        )
        .unwrap();

        let luminance = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<f32>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("luminance".into()),
            },
        )
        .unwrap();

        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "./adaptive_lum_histogram_gen.comp.spv"
                )),
                debug_name: Some("histogram_gen_shader".into()),
            },
        )
        .unwrap();

        let histogram_gen_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.histogram_gen.clone()],
                module,
                work_group_size: (HISTOGRAM_GEN_BLOCK_SIZE, HISTOGRAM_GEN_BLOCK_SIZE, 1),
                push_constants_size: Some(
                    std::mem::size_of::<GpuAdaptiveLumHistogramGenPushConstants>() as u32,
                ),
                debug_name: Some("histogram_gen_pipeline".into()),
            },
        )
        .unwrap();

        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./adaptive_lum.comp.spv")),
                debug_name: Some("adaptive_luminance_shader".into()),
            },
        )
        .unwrap();

        let luminance_comp_pipeline =
            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![layouts.adaptive_lum.clone()],
                    module,
                    work_group_size: (ADAPTIVE_LUM_HISTOGRAM_SIZE as u32, 1, 1),
                    push_constants_size: Some(
                        std::mem::size_of::<GpuAdaptiveLumPushConstants>() as u32
                    ),
                    debug_name: Some("adaptive_luminance_pipeline".into()),
                },
            )
            .unwrap();

        let vert_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./graphics_effect.vert.spv")),
                debug_name: Some("tonemapping_vertex_shader".into()),
            },
        )
        .unwrap();

        let frag_module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./tonemapping.frag.spv")),
                debug_name: Some("tonemapping_fragment_shader".into()),
            },
        )
        .unwrap();

        let tonemapping_pipeline =
            GraphicsPipeline::new(
                ctx.clone(),
                GraphicsPipelineCreateInfo {
                    stages: ShaderStages::Traditional {
                        vertex: vert_module.clone(),
                        fragment: Some(frag_module),
                    },
                    layouts: vec![layouts.tonemapping.clone(), layouts.camera.clone()],
                    vertex_input: VertexInputState {
                        attributes: Vec::default(),
                        bindings: Vec::default(),
                        topology: PrimitiveTopology::TriangleList,
                    },
                    rasterization: RasterizationState {
                        polygon_mode: PolygonMode::Fill,
                        cull_mode: CullMode::None,
                        front_face: FrontFace::CounterClockwise,
                    },
                    depth_stencil: None,
                    color_blend: ColorBlendState {
                        attachments: vec![ColorBlendAttachment {
                            write_mask: ColorComponents::R
                                | ColorComponents::G
                                | ColorComponents::B
                                | ColorComponents::A,
                            blend: false,
                            ..Default::default()
                        }],
                    },
                    push_constants_size: Some(
                        std::mem::size_of::<GpuToneMappingPushConstants>() as u32
                    ),
                    debug_name: Some("tonemapping_pipeline".into()),
                },
            )
            .unwrap();

        let histogram_sets = std::array::from_fn(|frame| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.histogram_gen.clone(),
                    debug_name: Some(format!("histogram_gen_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: HISTOGRAM_GEN_SET_HISTOGRAM_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &histogram,
                    array_element: 0,
                },
            }]);

            set
        });

        let luminance_set = {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.adaptive_lum.clone(),
                    debug_name: Some("luminance_gen_set".into()),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: ADAPTIVE_LUM_SET_HISTOGRAM_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &histogram,
                        array_element: 0,
                    },
                },
                DescriptorSetUpdate {
                    binding: ADAPTIVE_LUM_SET_LUMINANCE_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &luminance,
                        array_element: 0,
                    },
                },
            ]);

            set
        };

        let tonemapping_sets = std::array::from_fn(|frame| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.tonemapping.clone(),
                    debug_name: Some(format!("tonemapping_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: TONEMAPPING_SET_LUMINANCE_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &luminance,
                    array_element: 0,
                },
            }]);

            set
        });

        Self {
            _histogram: histogram,
            _luminance: luminance,
            screen_size: (1, 1),
            histogram_gen_pipeline,
            histogram_sets,
            luminance_comp_pipeline,
            luminance_set,
            tonemapping_pipeline,
            tonemapping_sets,
        }
    }

    pub fn bind_bloom(&mut self, frame: Frame, bloom_image: &Texture) {
        self.tonemapping_sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: TONEMAPPING_SET_BLOOM_IMAGE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: bloom_image,
                array_element: 0,
                sampler: BLOOM_SAMPLE_FILTER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn bind_sun_shafts(&mut self, frame: Frame, sun_shafts_image: &Texture) {
        self.tonemapping_sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: TONEMAPPING_SET_SUN_SHAFTS_IMAGE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: sun_shafts_image,
                array_element: 0,
                sampler: BLOOM_SAMPLE_FILTER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn bind_images(&mut self, frame: Frame, src: &Texture, depth: &Texture) {
        self.screen_size = (src.dims().0, src.dims().1);

        self.histogram_sets[usize::from(frame)].update(&[DescriptorSetUpdate {
            binding: HISTOGRAM_GEN_SET_INPUT_TEXTURE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src,
                array_element: 0,
                sampler: HISTOGRAM_SRC_IMAGE_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);

        self.tonemapping_sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: TONEMAPPING_SET_SOURCE_IMAGE_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: src,
                    array_element: 0,
                    sampler: TONEMAPPING_SRC_IMAGE_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: TONEMAPPING_SET_SOURCE_DEPTH_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: depth,
                    array_element: 0,
                    sampler: TONEMAPPING_SRC_IMAGE_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
        ]);
    }

    pub fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
        dst: ColorAttachmentDestination<'a>,
        settings: &TonemappingSettings,
        dt: Duration,
    ) {
        let lum_diff = (settings.max_luminance - settings.min_luminance).max(0.0001);

        let histogram_params = [GpuAdaptiveLumHistogramGenPushConstants {
            min_log2_lum: settings.min_luminance,
            inv_log2_lum: 1.0 / lum_diff,
        }];

        let lum_params = [GpuAdaptiveLumPushConstants {
            min_log_lum: settings.min_luminance,
            log_lum_range: lum_diff,
            num_pixels: (self.screen_size.0 * self.screen_size.1) as f32,
            time_coeff: settings.auto_exposure_rate * dt.as_secs_f32(),
        }];

        let tonemapping_params = [GpuToneMappingPushConstants {
            exposure: settings.exposure,
            gamma: settings.gamma,
        }];

        // Adaptive luminance
        commands.compute_pass(
            &self.histogram_gen_pipeline,
            Some("adaptive_lum_histogram_gen"),
            |pass| {
                pass.bind_sets(0, vec![&self.histogram_sets[usize::from(frame)]]);
                pass.push_constants(bytemuck::cast_slice(&histogram_params));
                ComputePassDispatch::Inline(
                    self.screen_size.0.div_ceil(HISTOGRAM_GEN_BLOCK_SIZE),
                    self.screen_size.1.div_ceil(HISTOGRAM_GEN_BLOCK_SIZE),
                    1,
                )
            },
        );

        commands.compute_pass(
            &self.luminance_comp_pipeline,
            Some("adaptive_lum_compute"),
            |pass| {
                pass.bind_sets(0, vec![&self.luminance_set]);
                pass.push_constants(bytemuck::cast_slice(&lum_params));
                ComputePassDispatch::Inline(1, 1, 1)
            },
        );

        // Tonemapping pass
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    dst,
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }],
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: None,
                depth_stencil_resolve_attachment: None,
            },
            Some("tonemapping"),
            |pass| {
                pass.bind_pipeline(self.tonemapping_pipeline.clone());
                pass.bind_sets(
                    0,
                    vec![
                        &self.tonemapping_sets[usize::from(frame)],
                        camera.get_set(frame),
                    ],
                );
                pass.push_constants(bytemuck::cast_slice(&tonemapping_params));
                pass.draw(3, 1, 0, 0);
            },
        );
    }
}
