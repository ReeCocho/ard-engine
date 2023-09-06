use ard_math::{IVec2, Vec2};
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use ordered_float::NotNan;

use crate::shader_constants::FRAMES_IN_FLIGHT;

const HZB_INPUT_IMAGE_BINDING: u32 = 0;
const HZB_OUTPUT_IMAGE_BINDING: u32 = 1;

const HZB_INPUT_SAMPLER: Sampler = Sampler {
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

pub(crate) struct HzbGlobal {
    ctx: Context,
    /// Layout for generating each mip.
    layout: DescriptorSetLayout,
    /// Pipeline to perform mip generation.
    pipeline: ComputePipeline,
}

pub(crate) struct HzbImage {
    /// The actual depth image.
    image: Texture,
    /// Dimensions of source images for this hzb.
    src_dims: (u32, u32, u32),
    /// Set for generating mip levels. One set per frame in flight.
    sets: [Vec<DescriptorSet>; FRAMES_IN_FLIGHT],
}

#[allow(dead_code)]
#[derive(Copy, Clone)]
struct HzbPushConstants {
    input_size: IVec2,
    inv_output_size: Vec2,
}

unsafe impl Pod for HzbPushConstants {}
unsafe impl Zeroable for HzbPushConstants {}

impl HzbGlobal {
    pub fn new(ctx: &Context) -> Self {
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    DescriptorBinding {
                        binding: HZB_INPUT_IMAGE_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    DescriptorBinding {
                        binding: HZB_OUTPUT_IMAGE_BINDING,
                        ty: DescriptorType::StorageImage(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/highz_gen.comp.spv"),
                debug_name: Some(String::from("hzb_gen_shader")),
            },
        )
        .unwrap();

        let pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layout.clone()],
                module,
                work_group_size: (2, 2, 1),
                push_constants_size: Some(std::mem::size_of::<HzbPushConstants>() as u32),
                debug_name: Some(String::from("hzb_gen_pipeline")),
            },
        )
        .unwrap();

        Self {
            ctx: ctx.clone(),
            layout,
            pipeline,
        }
    }

    pub fn new_image(&self, width: u32, height: u32) -> HzbImage {
        let mip_levels = (width.max(height) as f32).log2().floor() as usize;
        let image = Texture::new(
            self.ctx.clone(),
            TextureCreateInfo {
                format: Format::R32Sfloat,
                ty: TextureType::Type2D,
                width: (width / 2).max(1),
                height: (height / 2).max(1),
                depth: 1,
                array_elements: 1,
                mip_levels,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("hzb_image")),
            },
        )
        .unwrap();

        let mut sets: [Vec<DescriptorSet>; FRAMES_IN_FLIGHT] = Default::default();
        for set in &mut sets {
            let mut mip_sets = Vec::with_capacity(mip_levels);
            for i in 0..mip_levels {
                let mut mip_set = DescriptorSet::new(
                    self.ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: self.layout.clone(),
                        debug_name: Some(format!("hzb_mip_set_{i}")),
                    },
                )
                .unwrap();

                // Write in the images
                if i == 0 {
                    mip_set.update(&[DescriptorSetUpdate {
                        binding: HZB_OUTPUT_IMAGE_BINDING,
                        array_element: 0,
                        value: DescriptorValue::StorageImage {
                            texture: &image,
                            array_element: 0,
                            mip: 0,
                        },
                    }]);
                } else {
                    mip_set.update(&[
                        DescriptorSetUpdate {
                            binding: HZB_INPUT_IMAGE_BINDING,
                            array_element: 0,
                            value: DescriptorValue::Texture {
                                texture: &image,
                                array_element: 0,
                                sampler: HZB_INPUT_SAMPLER,
                                base_mip: i - 1,
                                mip_count: 1,
                            },
                        },
                        DescriptorSetUpdate {
                            binding: HZB_OUTPUT_IMAGE_BINDING,
                            array_element: 0,
                            value: DescriptorValue::StorageImage {
                                texture: &image,
                                array_element: 0,
                                mip: i,
                            },
                        },
                    ]);
                }

                mip_sets.push(mip_set);
            }

            *set = mip_sets;
        }

        HzbImage {
            image,
            sets,
            src_dims: (width, height, 1),
        }
    }

    pub fn generate<'a>(
        &self,
        frame: usize,
        command_buffer: &mut CommandBuffer<'a>,
        image: &'a mut HzbImage,
        src: &'a Texture,
    ) {
        assert_eq!(
            image.src_dims,
            src.dims(),
            "source image must be the same size as the HZB"
        );

        // Bind the source image to the HZB image
        image.sets[frame][0].update(&[DescriptorSetUpdate {
            binding: HZB_INPUT_IMAGE_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: src,
                array_element: 0,
                sampler: HZB_INPUT_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);

        // Generate the HZB
        command_buffer.compute_pass(|pass| {
            // Bind the pipeline for hzb gen
            pass.bind_pipeline(self.pipeline.clone());

            // Perform a dispatch for each mip level
            for (i, set) in image.sets[frame].iter().enumerate() {
                // Determine the size of this mip level
                let (mut src_width, mut src_height, _) = src.dims();
                let dst_width = (src_width >> (i + 1)).max(1);
                let dst_height = (src_height >> (i + 1)).max(1);
                src_width = (src_width >> i).max(1);
                src_height = (src_height >> i).max(1);

                // Determine the conversion factor for texels
                let constants = [HzbPushConstants {
                    input_size: IVec2::new(src_width as i32, src_height as i32),
                    inv_output_size: 1.0 / Vec2::new(dst_width as f32, dst_height as f32),
                }];

                // Send constants and dispatch
                pass.bind_sets(0, vec![set]);
                pass.push_constants(bytemuck::cast_slice(&constants));
                pass.dispatch(dst_width, dst_height, 1);
            }
        });
    }
}

impl HzbImage {
    #[inline(always)]
    pub fn texture(&self) -> &Texture {
        &self.image
    }
}
