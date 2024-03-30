use ard_ecs::prelude::*;
use ard_pal::prelude::*;

use crate::CameraClearColor;

#[derive(Component)]
pub struct RenderTarget {
    dims: (u32, u32),
    samples: MultiSamples,
    attachments: Attachments,
}

struct Attachments {
    color_target: Texture,
    color_resolve: Texture,
    depth_target: Texture,
    depth_resolve: Texture,
    thin_g_target: Texture,
    thin_g_resolve: Texture,
    linear_color: Texture,
}

impl RenderTarget {
    pub const COLOR_TARGET_FORMAT: Format = Format::Rgba16SFloat;
    pub const THIN_G_TARGET_FORMAT: Format = Format::Rgba8Snorm;
    pub const FINAL_COLOR_FORMAT: Format = Format::Rgba8Unorm;
    pub const DEPTH_FORMAT: Format = Format::D32Sfloat;

    pub fn new(ctx: &Context, dims: (u32, u32), samples: MultiSamples) -> Self {
        debug_assert!(dims.0 > 0 && dims.1 > 0);

        let attachments = Self::create_images(ctx, dims, samples);

        Self {
            attachments,
            dims,
            samples,
        }
    }

    pub fn hzb_pass(&self) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: Vec::default(),
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: self.final_depth(),
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                store_op: StoreOp::Store,
                samples: MultiSamples::Count1,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn depth_prepass(&self) -> RenderPassDescriptor {
        let (load_op, dsra) = match self.samples {
            // We can load the result of the HZB render when not using multi-sampling.
            MultiSamples::Count1 => (LoadOp::Load, None),
            _ => (
                LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                Some(DepthStencilResolveAttachment {
                    dst: DepthStencilAttachmentDestination::Texture {
                        texture: &self.attachments.depth_resolve,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    depth_resolve_mode: ResolveMode::Min,
                    stencil_resolve_mode: ResolveMode::SampleZero,
                }),
            ),
        };

        let (store_op, tgra) = match self.samples {
            MultiSamples::Count1 => (StoreOp::Store, Vec::default()),
            _ => (
                StoreOp::DontCare,
                vec![ColorResolveAttachment {
                    src: 0,
                    dst: ColorAttachmentDestination::Texture {
                        texture: &self.attachments.thin_g_resolve,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                }],
            ),
        };

        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, -1.0)),
                store_op,
                samples: self.samples,
                dst: ColorAttachmentDestination::Texture {
                    texture: &self.attachments.thin_g_target,
                    array_element: 0,
                    mip_level: 0,
                },
            }],
            color_resolve_attachments: tgra,
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: &self.attachments.depth_target,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op,
                store_op: StoreOp::Store,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: dsra,
        }
    }

    pub fn copy_depth<'a>(&'a self, commands: &mut CommandBuffer<'a>) {
        // Only need to copy if we aren't using MSAA
        if self.samples != MultiSamples::Count1 {
            return;
        }

        commands.copy_texture_to_texture(CopyTextureToTexture {
            src: &self.attachments.depth_target,
            src_offset: (0, 0, 0),
            src_mip_level: 0,
            src_array_element: 0,
            dst: &self.attachments.depth_resolve,
            dst_offset: (0, 0, 0),
            dst_mip_level: 0,
            dst_array_element: 0,
            extent: self.attachments.depth_target.dims(),
        });
    }

    pub fn opaque_pass(&self, clear_op: CameraClearColor) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                dst: ColorAttachmentDestination::Texture {
                    texture: &self.attachments.color_target,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: match clear_op {
                    CameraClearColor::None => LoadOp::DontCare,
                    CameraClearColor::Color(color) => {
                        LoadOp::Clear(ClearColor::RgbaF32(color.x, color.y, color.z, color.w))
                    }
                },
                store_op: StoreOp::Store,
                samples: self.samples,
            }],
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: &self.attachments.depth_target,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: StoreOp::None,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn transparent_pass(&self) -> RenderPassDescriptor {
        let cra = match self.samples {
            MultiSamples::Count1 => Vec::default(),
            _ => vec![ColorResolveAttachment {
                src: 0,
                dst: ColorAttachmentDestination::Texture {
                    texture: &self.attachments.color_resolve,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
        };

        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                dst: ColorAttachmentDestination::Texture {
                    texture: &self.attachments.color_target,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
                samples: self.samples,
            }],
            color_resolve_attachments: cra,
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: &self.attachments.depth_target,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: StoreOp::DontCare,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    #[inline(always)]
    pub fn dims(&self) -> (u32, u32) {
        self.dims
    }

    #[inline(always)]
    pub fn samples(&self) -> MultiSamples {
        self.samples
    }

    #[inline(always)]
    pub fn color_target(&self) -> &Texture {
        &self.attachments.color_target
    }

    #[inline(always)]
    pub fn final_color(&self) -> &Texture {
        match self.samples {
            MultiSamples::Count1 => &self.attachments.color_target,
            _ => &self.attachments.color_resolve,
        }
    }

    #[inline(always)]
    pub fn linear_color(&self) -> &Texture {
        &self.attachments.linear_color
    }

    #[inline(always)]
    pub fn depth_target(&self) -> &Texture {
        &self.attachments.depth_target
    }

    #[inline(always)]
    pub fn depth_resolve(&self) -> &Texture {
        &self.attachments.depth_resolve
    }

    #[inline(always)]
    pub fn final_depth(&self) -> &Texture {
        match self.samples {
            MultiSamples::Count1 => &self.attachments.depth_target,
            _ => &self.attachments.depth_resolve,
        }
    }

    fn create_images(ctx: &Context, dims: (u32, u32), samples: MultiSamples) -> Attachments {
        Attachments {
            color_target: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::COLOR_TARGET_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: samples,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("color_target".to_owned()),
                },
            )
            .unwrap(),
            color_resolve: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::COLOR_TARGET_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("color_resolve".to_owned()),
                },
            )
            .unwrap(),
            depth_target: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::DEPTH_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: samples,
                    texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("depth_target".to_owned()),
                },
            )
            .unwrap(),
            depth_resolve: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::DEPTH_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_DST,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("depth_resolve".to_owned()),
                },
            )
            .unwrap(),
            thin_g_target: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::THIN_G_TARGET_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: samples,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("thin_g_target".to_owned()),
                },
            )
            .unwrap(),
            thin_g_resolve: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::THIN_G_TARGET_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("thin_g_resolve".to_owned()),
                },
            )
            .unwrap(),
            linear_color: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: Self::FINAL_COLOR_FORMAT,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::COLOR_ATTACHMENT
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("linear_color".to_owned()),
                },
            )
            .unwrap(),
        }
    }
}
