use ard_ecs::prelude::*;
use ard_pal::prelude::*;

use crate::CameraClearColor;

#[derive(Component)]
pub struct RenderTarget {
    ctx: Context,
    dims: (u32, u32),
    samples: MultiSamples,
    color: Texture,
    color_resolve: Option<Texture>,
    depth: Texture,
    depth_resolve: Option<Texture>,
}

impl RenderTarget {
    pub const COLOR_FORMAT: Format = Format::Rgba16SFloat;
    pub const DEPTH_FORMAT: Format = Format::D32Sfloat;

    pub fn new(ctx: &Context, dims: (u32, u32), samples: MultiSamples) -> Self {
        debug_assert!(dims.0 > 0 && dims.1 > 0);

        let (color, color_resolve, depth, depth_resolve) = Self::create_images(ctx, dims, samples);

        Self {
            ctx: ctx.clone(),
            color,
            color_resolve,
            depth,
            depth_resolve,
            dims,
            samples,
        }
    }

    pub fn hzb_pass(&self) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: Vec::default(),
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: self.depth(),
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                store_op: StoreOp::Store,
                samples: MultiSamples::Count1,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn depth_prepass(&self) -> RenderPassDescriptor {
        if let Some(resolve_dst) = &self.depth_resolve {
            RenderPassDescriptor {
                color_attachments: Vec::default(),
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    texture: &self.depth,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                    store_op: StoreOp::Store,
                    samples: self.samples,
                }),
                // Store and resolve depth for image effects
                depth_stencil_resolve_attachment: Some(DepthStencilResolveAttachment {
                    dst: resolve_dst,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    depth_resolve_mode: ResolveMode::Max,
                    stencil_resolve_mode: ResolveMode::Max,
                }),
            }
        } else {
            RenderPassDescriptor {
                color_attachments: Vec::default(),
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    texture: &self.depth,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                    samples: self.samples,
                }),
                depth_stencil_resolve_attachment: None,
            }
        }
    }

    pub fn opaque_pass(&self, clear_op: CameraClearColor) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::Texture {
                    texture: &self.color,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: match clear_op {
                    CameraClearColor::None => LoadOp::Load,
                    CameraClearColor::Color(color) => {
                        LoadOp::Clear(ClearColor::RgbaF32(color.x, color.y, color.z, color.w))
                    }
                },
                store_op: StoreOp::Store,
                samples: self.samples,
            }],
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: &self.depth,
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn transparent_pass(&self) -> RenderPassDescriptor {
        if let Some(color_resolve) = &self.color_resolve {
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    source: ColorAttachmentSource::Texture {
                        texture: &self.color,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                    samples: self.samples,
                }],
                color_resolve_attachments: vec![ColorResolveAttachment {
                    src: 0,
                    dst: ColorAttachmentSource::Texture {
                        texture: color_resolve,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                }],
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    texture: &self.depth,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::Load,
                    store_op: StoreOp::DontCare,
                    samples: self.samples,
                }),
                depth_stencil_resolve_attachment: None,
            }
        } else {
            RenderPassDescriptor {
                color_attachments: vec![ColorAttachment {
                    source: ColorAttachmentSource::Texture {
                        texture: &self.color,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                    samples: self.samples,
                }],
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    texture: &self.depth,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::Load,
                    store_op: StoreOp::DontCare,
                    samples: self.samples,
                }),
                depth_stencil_resolve_attachment: None,
            }
        }
    }

    #[inline(always)]
    pub fn dims(&self) -> (u32, u32) {
        self.dims
    }

    #[inline(always)]
    pub fn color(&self) -> &Texture {
        self.color_resolve.as_ref().unwrap_or(&self.color)
    }

    #[inline(always)]
    pub fn depth(&self) -> &Texture {
        self.depth_resolve.as_ref().unwrap_or(&self.depth)
    }

    pub fn resize(&mut self, dims: (u32, u32), samples: MultiSamples) {
        debug_assert!(dims.0 > 0 && dims.1 > 0);

        self.dims = dims;
        self.samples = samples;

        let (color, color_resolve, depth, depth_resolve) =
            Self::create_images(&self.ctx, dims, samples);

        self.color = color;
        self.color_resolve = color_resolve;
        self.depth = depth;
        self.depth_resolve = depth_resolve;
    }

    fn create_images(
        ctx: &Context,
        dims: (u32, u32),
        samples: MultiSamples,
    ) -> (Texture, Option<Texture>, Texture, Option<Texture>) {
        let color = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::COLOR_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: samples,
                texture_usage: if samples == MultiSamples::Count1 {
                    TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED
                } else {
                    TextureUsage::COLOR_ATTACHMENT | TextureUsage::TRANSFER_SRC
                },
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("color_target".to_owned()),
            },
        )
        .unwrap();

        let depth = Texture::new(
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
                texture_usage: if samples == MultiSamples::Count1 {
                    TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED
                } else {
                    TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::TRANSFER_SRC
                },
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("depth_target".to_owned()),
            },
        )
        .unwrap();

        let color_resolve = if samples != MultiSamples::Count1 {
            Some(
                Texture::new(
                    ctx.clone(),
                    TextureCreateInfo {
                        format: Self::COLOR_FORMAT,
                        ty: TextureType::Type2D,
                        width: dims.0,
                        height: dims.1,
                        depth: 1,
                        array_elements: 1,
                        mip_levels: 1,
                        sample_count: MultiSamples::Count1,
                        texture_usage: TextureUsage::COLOR_ATTACHMENT
                            | TextureUsage::SAMPLED
                            | TextureUsage::TRANSFER_DST,
                        memory_usage: MemoryUsage::GpuOnly,
                        queue_types: QueueTypes::MAIN,
                        sharing_mode: SharingMode::Exclusive,
                        debug_name: Some("color_resolve_target".to_owned()),
                    },
                )
                .unwrap(),
            )
        } else {
            None
        };

        let depth_resolve = if samples != MultiSamples::Count1 {
            Some(
                Texture::new(
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
                        queue_types: QueueTypes::MAIN,
                        sharing_mode: SharingMode::Exclusive,
                        debug_name: Some("depth_resolve_target".to_owned()),
                    },
                )
                .unwrap(),
            )
        } else {
            None
        };

        (color, color_resolve, depth, depth_resolve)
    }
}
