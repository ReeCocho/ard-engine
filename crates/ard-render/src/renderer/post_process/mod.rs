use ard_math::Vec2;
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use ordered_float::NotNan;

use crate::shader_constants::FRAMES_IN_FLIGHT;

use self::adaptive_luminance::AdaptiveLuminance;

pub mod adaptive_luminance;

const SCREEN_SAMPLER: Sampler = Sampler {
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
    border_color: None,
    unnormalize_coords: false,
};

#[derive(Copy, Clone)]
pub struct PostProcessingSettings {
    pub exposure: f32,
    pub fxaa: bool,
}

pub(crate) struct PostProcessing {
    ctx: Context,
    adaptive_lum: AdaptiveLuminance,
    _layout: DescriptorSetLayout,
    _lum_layout: DescriptorSetLayout,
    sets: Vec<PostProcessingSets>,
    tonemapping_pipeline: GraphicsPipeline,
    fxaa_pipeline: GraphicsPipeline,
    /// 2 LDR images to ping-ping between when applying post processing.
    images: Texture,
}

struct PostProcessingSets {
    /// Source HDR image to sample.
    src: DescriptorSet,
    /// Set to hold luminance value.
    lum: DescriptorSet,
    /// Image to render while sampling `src` or `pong`.
    ping: DescriptorSet,
    /// Image to render to while sampling `ping`.
    pong: DescriptorSet,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PostProcessingPushConstants {
    pub screen_size: Vec2,
    pub exposure: f32,
    pub fxaa_enabled: u32,
}

unsafe impl Pod for PostProcessingPushConstants {}
unsafe impl Zeroable for PostProcessingPushConstants {}

impl PostProcessing {
    pub fn new(ctx: &Context, width: u32, height: u32) -> Self {
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![DescriptorBinding {
                    binding: 0,
                    ty: DescriptorType::Texture,
                    count: 1,
                    stage: ShaderStage::Fragment,
                }],
            },
        )
        .unwrap();

        let lum_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![DescriptorBinding {
                    binding: 0,
                    ty: DescriptorType::StorageBuffer(AccessType::Read),
                    count: 1,
                    stage: ShaderStage::Fragment,
                }],
            },
        )
        .unwrap();

        let adaptive_lum = AdaptiveLuminance::new(ctx);

        let post_proc_images = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Bgra8Unorm,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 2,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("post_proc_images")),
            },
        )
        .unwrap();

        let mut sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for i in 0..FRAMES_IN_FLIGHT {
            let src = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layout.clone(),
                    debug_name: Some(format!("post_processing_src_set_{i}")),
                },
            )
            .unwrap();

            let mut ping = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layout.clone(),
                    debug_name: Some(format!("post_processing_ping_set_{i}")),
                },
            )
            .unwrap();

            ping.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &post_proc_images,
                    array_element: 1,
                    sampler: SCREEN_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);

            let mut pong = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layout.clone(),
                    debug_name: Some(format!("post_processing_pong_set_{i}")),
                },
            )
            .unwrap();

            pong.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &post_proc_images,
                    array_element: 0,
                    sampler: SCREEN_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);

            let mut lum = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: lum_layout.clone(),
                    debug_name: Some(format!("post_processing_lum_set_{i}")),
                },
            )
            .unwrap();

            lum.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: adaptive_lum.luminance(),
                    array_element: 0,
                },
            }]);

            sets.push(PostProcessingSets {
                src,
                ping,
                pong,
                lum,
            });
        }

        let vertex = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../../shaders/post_process.vert.spv"),
                debug_name: Some(String::from("post_processing_vertex_shader")),
            },
        )
        .unwrap();

        let tonemapping = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../../shaders/tonemapping.frag.spv"),
                debug_name: Some(String::from("tonemapping_fragment_shader")),
            },
        )
        .unwrap();

        let fxaa = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../../shaders/fxaa.frag.spv"),
                debug_name: Some(String::from("fxaa_fragment_shader")),
            },
        )
        .unwrap();

        let tonemapping_pipeline =
            GraphicsPipeline::new(
                ctx.clone(),
                GraphicsPipelineCreateInfo {
                    stages: ShaderStages {
                        vertex: vertex.clone(),
                        fragment: Some(tonemapping),
                    },
                    layouts: vec![layout.clone(), lum_layout.clone()],
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
                    color_blend: Some(ColorBlendState {
                        attachments: vec![ColorBlendAttachment {
                            write_mask: ColorComponents::R
                                | ColorComponents::G
                                | ColorComponents::B
                                | ColorComponents::A,
                            blend: false,
                            ..Default::default()
                        }],
                    }),
                    push_constants_size: Some(
                        std::mem::size_of::<PostProcessingPushConstants>() as u32
                    ),
                    debug_name: Some(String::from("tonemapping_pipeline")),
                },
            )
            .unwrap();

        let fxaa_pipeline =
            GraphicsPipeline::new(
                ctx.clone(),
                GraphicsPipelineCreateInfo {
                    stages: ShaderStages {
                        vertex,
                        fragment: Some(fxaa),
                    },
                    layouts: vec![layout.clone()],
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
                    color_blend: Some(ColorBlendState {
                        attachments: vec![ColorBlendAttachment {
                            write_mask: ColorComponents::R
                                | ColorComponents::G
                                | ColorComponents::B
                                | ColorComponents::A,
                            blend: false,
                            ..Default::default()
                        }],
                    }),
                    push_constants_size: Some(
                        std::mem::size_of::<PostProcessingPushConstants>() as u32
                    ),
                    debug_name: Some(String::from("fxaa_pipeline")),
                },
            )
            .unwrap();

        Self {
            ctx: ctx.clone(),
            adaptive_lum,
            _layout: layout,
            _lum_layout: lum_layout,
            sets,
            tonemapping_pipeline,
            fxaa_pipeline,
            images: post_proc_images,
        }
    }

    #[inline(always)]
    pub fn final_image(&self) -> (&Texture, usize) {
        (&self.images, 1)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.images = Texture::new(
            self.ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Bgra8Unorm,
                ty: TextureType::Type2D,
                width,
                height,
                depth: 1,
                array_elements: 2,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("post_proc_images")),
            },
        )
        .unwrap();

        for frame in 0..FRAMES_IN_FLIGHT {
            let sets = &mut self.sets[frame];
            sets.ping.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &self.images,
                    array_element: 1,
                    sampler: SCREEN_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);

            sets.pong.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &self.images,
                    array_element: 0,
                    sampler: SCREEN_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);
        }
    }

    pub fn prepare(&mut self, frame: usize, image: &Texture) {
        self.sets[frame].src.update(&[DescriptorSetUpdate {
            binding: 0,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: image,
                array_element: 0,
                sampler: SCREEN_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);

        self.adaptive_lum.update_set(frame, image);
    }

    pub fn draw<'a, 'b>(
        &'a self,
        frame: usize,
        canvas_size: Vec2,
        settings: &PostProcessingSettings,
        commands: &'b mut CommandBuffer<'a>,
    ) {
        let constants = [PostProcessingPushConstants {
            screen_size: canvas_size,
            exposure: settings.exposure,
            fxaa_enabled: settings.fxaa as u32,
        }];

        // Compute luminance
        self.adaptive_lum.compute(frame, commands);

        // First pass: Tonemapping.
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    source: ColorAttachmentSource::Texture {
                        texture: &self.images,
                        // Ping
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                }],
                depth_stencil_attachment: None,
            },
            |pass| {
                pass.bind_pipeline(self.tonemapping_pipeline.clone());
                pass.bind_sets(0, vec![&self.sets[frame].src, &self.sets[frame].lum]);
                pass.push_constants(bytemuck::cast_slice(&constants));
                pass.draw(3, 1, 0, 0);
            },
        );

        // Second pass: FXAA.
        commands.render_pass(
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    source: ColorAttachmentSource::Texture {
                        texture: &self.images,
                        // Pong
                        array_element: 1,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                }],
                depth_stencil_attachment: None,
            },
            |pass| {
                pass.bind_pipeline(self.fxaa_pipeline.clone());
                pass.bind_sets(0, vec![&self.sets[frame].pong]);
                pass.push_constants(bytemuck::cast_slice(&constants));
                pass.draw(3, 1, 0, 0);
            },
        );
    }
}

impl Default for PostProcessingSettings {
    fn default() -> Self {
        PostProcessingSettings {
            exposure: 0.3,
            fxaa: true,
        }
    }
}
