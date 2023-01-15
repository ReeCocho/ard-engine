use ard_math::Vec4;
use ard_pal::prelude::*;
use ordered_float::NotNan;

use crate::shader_constants::FRAMES_IN_FLIGHT;

const BLOCK_SIZE: usize = 16;
const LUMINANCE_HISTOGRAM_SIZE: usize = 256;

const HISTOGRAM_SRC_BINDING: u32 = 0;
const HISTOGRAM_DST_BINDING: u32 = 1;

const LUMINANCE_HISTOGRAM_BINDING: u32 = 0;
const LUMINANCE_DST_BINDING: u32 = 1;

const HISTOGRAM_SRC_SAMPLER: Sampler = Sampler {
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

pub(crate) struct AdaptiveLuminance {
    /// Pipeline for generating the luminance histogram.
    histogram_pipeline: ComputePipeline,
    /// Pipeline for computing luminance.
    luminance_pipeline: ComputePipeline,
    /// Layout for generating the histogram.
    _histogram_layout: DescriptorSetLayout,
    histogram_sets: Vec<DescriptorSet>,
    /// Layout for computing luminance.
    _lum_layout: DescriptorSetLayout,
    lum_sets: Vec<DescriptorSet>,
    /// Buffer to hold the luminance histogram.
    _luminance_histogram: Buffer,
    /// Buffer to hold the computed luminance value.
    luminance: Buffer,
    /// Size of the image to compute luminance for.
    src_size: (u32, u32),
}

impl AdaptiveLuminance {
    pub fn new(ctx: &Context) -> Self {
        let luminance_histogram = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (LUMINANCE_HISTOGRAM_SIZE * std::mem::size_of::<u32>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("luminance_histogram")),
            },
        )
        .unwrap();

        let luminance = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<u32>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("luminance")),
            },
        )
        .unwrap();

        let histogram_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    DescriptorBinding {
                        binding: HISTOGRAM_SRC_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: HISTOGRAM_DST_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let histogram_pipeline = {
            let module = Shader::new(
                ctx.clone(),
                ShaderCreateInfo {
                    code: include_bytes!("../../shaders/lum_histogram.comp.spv"),
                    debug_name: Some(String::from("histogram_gen_shader")),
                },
            )
            .unwrap();

            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![histogram_layout.clone()],
                    module,
                    work_group_size: (BLOCK_SIZE as u32, BLOCK_SIZE as u32, 1),
                    push_constants_size: Some(std::mem::size_of::<Vec4>() as u32),
                    debug_name: Some(String::from("histogram_gen_pipeline")),
                },
            )
            .unwrap()
        };

        let lum_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    DescriptorBinding {
                        binding: LUMINANCE_HISTOGRAM_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: LUMINANCE_DST_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let luminance_pipeline = {
            let module = Shader::new(
                ctx.clone(),
                ShaderCreateInfo {
                    code: include_bytes!("../../shaders/luminance.comp.spv"),
                    debug_name: Some(String::from("luminance_gen_shader")),
                },
            )
            .unwrap();

            ComputePipeline::new(
                ctx.clone(),
                ComputePipelineCreateInfo {
                    layouts: vec![lum_layout.clone()],
                    module,
                    work_group_size: (LUMINANCE_HISTOGRAM_SIZE as u32, 1, 1),
                    push_constants_size: Some(std::mem::size_of::<Vec4>() as u32),
                    debug_name: Some(String::from("luminance_gen_pipeline")),
                },
            )
            .unwrap()
        };

        let histogram_sets = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|frame| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: histogram_layout.clone(),
                        debug_name: Some(format!("histogram_gen_set_{frame}")),
                    },
                )
                .unwrap();

                set.update(&[DescriptorSetUpdate {
                    binding: HISTOGRAM_DST_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &luminance_histogram,
                        array_element: 0,
                    },
                }]);

                set
            })
            .collect();

        let lum_sets = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|frame| {
                let mut set = DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: lum_layout.clone(),
                        debug_name: Some(format!("luminance_gen_set_{frame}")),
                    },
                )
                .unwrap();

                set.update(&[
                    DescriptorSetUpdate {
                        binding: LUMINANCE_DST_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &luminance,
                            array_element: 0,
                        },
                    },
                    DescriptorSetUpdate {
                        binding: LUMINANCE_HISTOGRAM_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageBuffer {
                            buffer: &luminance_histogram,
                            array_element: 0,
                        },
                    },
                ]);

                set
            })
            .collect();

        Self {
            histogram_pipeline,
            luminance_pipeline,
            _histogram_layout: histogram_layout,
            histogram_sets,
            _lum_layout: lum_layout,
            lum_sets,
            _luminance_histogram: luminance_histogram,
            luminance,
            src_size: (0, 0),
        }
    }

    #[inline(always)]
    pub fn luminance(&self) -> &Buffer {
        &self.luminance
    }

    pub fn update_set(&mut self, frame: usize, src: &Texture) {
        let (width, height, _) = src.dims();
        self.src_size = (width, height);

        self.histogram_sets[frame].update(&[DescriptorSetUpdate {
            binding: HISTOGRAM_SRC_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src,
                array_element: 0,
                sampler: HISTOGRAM_SRC_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);
    }

    pub fn compute<'a, 'b>(&'a self, frame: usize, commands: &'b mut CommandBuffer<'a>) {
        const MIN_LOG_LUM: f32 = -5.0;
        const MAX_LOG_LUM: f32 = 4.0;

        let histogram_params = [Vec4::new(
            MIN_LOG_LUM,
            1.0 / (MAX_LOG_LUM - MIN_LOG_LUM),
            0.0,
            0.0,
        )];

        let lum_params = [Vec4::new(
            MIN_LOG_LUM,
            MAX_LOG_LUM - MIN_LOG_LUM,
            0.1,
            (self.src_size.0 * self.src_size.1) as f32,
        )];

        commands.compute_pass(|pass| {
            // Generate the histogram
            pass.bind_pipeline(self.histogram_pipeline.clone());
            pass.bind_sets(0, vec![&self.histogram_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&histogram_params));
            pass.dispatch(
                (self.src_size.0 as f32 / BLOCK_SIZE as f32).ceil() as u32,
                (self.src_size.1 as f32 / BLOCK_SIZE as f32).ceil() as u32,
                1,
            );

            // Compute luminance
            pass.bind_pipeline(self.luminance_pipeline.clone());
            pass.bind_sets(0, vec![&self.lum_sets[frame]]);
            pass.push_constants(bytemuck::cast_slice(&lum_params));
            pass.dispatch(1, 1, 1);
        });
    }
}
