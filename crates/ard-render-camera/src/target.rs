use ard_ecs::prelude::*;
use ard_pal::prelude::*;

use crate::CameraClearColor;

#[derive(Component)]
pub struct RenderTarget {
    dims: (u32, u32),
    samples: MultiSamples,
    attachments: Attachments,
}

enum Attachments {
    SingleSample {
        color_target: Texture,
        depth_target: Texture,
        thin_g_target: Texture,
        vel_target: Texture,
        norm_target: Texture,
        linear_color: Texture,
        entities: Texture,
    },
    MultiSample {
        color_target: Texture,
        color_resolve: Texture,
        depth_target: Texture,
        depth_resolve: Texture,
        thin_g_target: Texture,
        thin_g_resolve: Texture,
        vel_target: Texture,
        vel_resolve: Texture,
        norm_target: Texture,
        norm_resolve: Texture,
        linear_color: Texture,
        entities: Texture,
    },
}

impl RenderTarget {
    pub const COLOR_TARGET_FORMAT: Format = Format::Rgba16SFloat;
    /// rgb  => `kS` factor for light apply.
    /// a    => Material roughness.
    pub const THIN_G_TARGET_FORMAT: Format = Format::Rgba8Unorm;
    /// UV-space velocities.
    pub const VEL_TARGET_FORMAT: Format = Format::Rg16SFloat;
    /// World space normals
    pub const NORM_TARGET_FORMAT: Format = Format::Rgba8Snorm;
    pub const FINAL_COLOR_FORMAT: Format = Format::Rgba8Unorm;
    pub const DEPTH_FORMAT: Format = Format::D32Sfloat;
    pub const ENTITIES_FORMAT: Format = Format::R32UInt;

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
        let (dt, load_op, dsra) = match &self.attachments {
            Attachments::SingleSample { depth_target, .. } => (
                depth_target,
                // We can load the result of the HZB render when not using multi-sampling.
                LoadOp::Load,
                None,
            ),
            Attachments::MultiSample {
                depth_target,
                depth_resolve,
                ..
            } => (
                depth_target,
                LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                Some(DepthStencilResolveAttachment {
                    dst: DepthStencilAttachmentDestination::Texture {
                        texture: depth_resolve,
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

        RenderPassDescriptor {
            color_attachments: Vec::default(),
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: dt,
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

    // Copy depth buffer for use by the opaque pass.
    pub fn copy_depth<'a>(&'a self, commands: &mut CommandBuffer<'a>) {
        let dt = match &self.attachments {
            Attachments::SingleSample { depth_target, .. } => depth_target,
            Attachments::MultiSample { depth_target, .. } => depth_target,
        };

        commands.copy_texture_to_texture(CopyTextureToTexture {
            src: dt,
            src_offset: (0, 0, 0),
            src_mip_level: 0,
            src_array_element: 0,
            dst: dt,
            dst_offset: (0, 0, 0),
            dst_mip_level: 0,
            dst_array_element: 1,
            extent: dt.dims(),
        });
    }

    pub fn entities_pass(&self) -> RenderPassDescriptor {
        let entities = match &self.attachments {
            Attachments::SingleSample { entities, .. } => entities,
            Attachments::MultiSample { entities, .. } => entities,
        };

        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                dst: ColorAttachmentDestination::Texture {
                    texture: entities,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Clear(ClearColor::RU32(u32::from(Entity::null()))),
                store_op: StoreOp::Store,
                samples: MultiSamples::Count1,
            }],
            color_resolve_attachments: Vec::default(),
            depth_stencil_attachment: None,
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn opaque_pass(&self, clear_op: CameraClearColor) -> RenderPassDescriptor {
        let (col, thin_g, vel, norm, gstore, depth, resolve) = match &self.attachments {
            Attachments::SingleSample {
                color_target,
                thin_g_target,
                vel_target,
                norm_target,
                depth_target,
                ..
            } => (
                color_target,
                thin_g_target,
                vel_target,
                norm_target,
                StoreOp::Store,
                depth_target,
                Vec::default(),
            ),
            Attachments::MultiSample {
                color_target,
                depth_target,
                thin_g_target,
                thin_g_resolve,
                vel_target,
                vel_resolve,
                norm_target,
                norm_resolve,
                ..
            } => (
                color_target,
                thin_g_target,
                vel_target,
                norm_target,
                StoreOp::DontCare,
                depth_target,
                vec![
                    // NOTE: We only resolve G-buffer targets in this pass. Color is resolved
                    // during the transparent pass.
                    ColorResolveAttachment {
                        src: 1,
                        dst: ColorAttachmentDestination::Texture {
                            texture: thin_g_resolve,
                            array_element: 0,
                            mip_level: 0,
                        },
                        load_op: LoadOp::DontCare,
                        store_op: StoreOp::Store,
                    },
                    ColorResolveAttachment {
                        src: 2,
                        dst: ColorAttachmentDestination::Texture {
                            texture: vel_resolve,
                            array_element: 0,
                            mip_level: 0,
                        },
                        load_op: LoadOp::DontCare,
                        store_op: StoreOp::Store,
                    },
                    ColorResolveAttachment {
                        src: 3,
                        dst: ColorAttachmentDestination::Texture {
                            texture: norm_resolve,
                            array_element: 0,
                            mip_level: 0,
                        },
                        load_op: LoadOp::DontCare,
                        store_op: StoreOp::Store,
                    },
                ],
            ),
        };

        RenderPassDescriptor {
            color_attachments: vec![
                // Color attachment
                ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: col,
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
                },
                // Thin-G attachment
                ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: thin_g,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                    store_op: gstore,
                    samples: self.samples,
                },
                // Vel attachment
                ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: vel,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                    store_op: gstore,
                    samples: self.samples,
                },
                // Norm attachment
                ColorAttachment {
                    dst: ColorAttachmentDestination::Texture {
                        texture: norm,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::RgbaF32(0.0, 0.0, 0.0, 0.0)),
                    store_op: gstore,
                    samples: self.samples,
                },
            ],
            color_resolve_attachments: resolve,
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: depth,
                    array_element: 1,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: StoreOp::DontCare,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: None,
        }
    }

    pub fn transparent_pass(&self) -> RenderPassDescriptor {
        let (color, color_resolve, depth, depth_resolve, store_op) = match &self.attachments {
            Attachments::SingleSample {
                color_target,
                depth_target,
                ..
            } => (
                color_target,
                Vec::default(),
                depth_target,
                None,
                StoreOp::Store,
            ),
            Attachments::MultiSample {
                color_target,
                color_resolve,
                depth_target,
                depth_resolve,
                ..
            } => (
                color_target,
                vec![ColorResolveAttachment {
                    src: 0,
                    dst: ColorAttachmentDestination::Texture {
                        texture: color_resolve,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                }],
                depth_target,
                Some(DepthStencilResolveAttachment {
                    dst: DepthStencilAttachmentDestination::Texture {
                        texture: depth_resolve,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::DontCare,
                    store_op: StoreOp::Store,
                    depth_resolve_mode: ResolveMode::Average,
                    stencil_resolve_mode: ResolveMode::SampleZero,
                }),
                StoreOp::DontCare,
            ),
        };

        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                dst: ColorAttachmentDestination::Texture {
                    texture: color,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: store_op,
                samples: self.samples,
            }],
            color_resolve_attachments: color_resolve,
            depth_stencil_attachment: Some(DepthStencilAttachment {
                dst: DepthStencilAttachmentDestination::Texture {
                    texture: depth,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op,
                samples: self.samples,
            }),
            depth_stencil_resolve_attachment: depth_resolve,
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
        match &self.attachments {
            Attachments::SingleSample { color_target, .. } => color_target,
            Attachments::MultiSample { color_target, .. } => color_target,
        }
    }

    #[inline(always)]
    pub fn final_thin_g(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { thin_g_target, .. } => thin_g_target,
            Attachments::MultiSample { thin_g_resolve, .. } => thin_g_resolve,
        }
    }

    #[inline(always)]
    pub fn final_vel(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { vel_target, .. } => vel_target,
            Attachments::MultiSample { vel_resolve, .. } => vel_resolve,
        }
    }

    #[inline(always)]
    pub fn final_norm(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { norm_target, .. } => norm_target,
            Attachments::MultiSample { norm_resolve, .. } => norm_resolve,
        }
    }

    #[inline(always)]
    pub fn vel_target(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { vel_target, .. } => vel_target,
            Attachments::MultiSample { vel_target, .. } => vel_target,
        }
    }

    #[inline(always)]
    pub fn norm_target(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { norm_target, .. } => norm_target,
            Attachments::MultiSample { norm_target, .. } => norm_target,
        }
    }

    #[inline(always)]
    pub fn entity_ids(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { entities, .. } => entities,
            Attachments::MultiSample { entities, .. } => entities,
        }
    }

    #[inline(always)]
    pub fn final_color(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { color_target, .. } => color_target,
            Attachments::MultiSample { color_resolve, .. } => color_resolve,
        }
    }

    #[inline(always)]
    pub fn linear_color(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { linear_color, .. } => linear_color,
            Attachments::MultiSample { linear_color, .. } => linear_color,
        }
    }

    #[inline(always)]
    pub fn final_depth(&self) -> &Texture {
        match &self.attachments {
            Attachments::SingleSample { depth_target, .. } => depth_target,
            Attachments::MultiSample { depth_resolve, .. } => depth_resolve,
        }
    }

    fn create_images(ctx: &Context, dims: (u32, u32), samples: MultiSamples) -> Attachments {
        let extra_usage = match samples {
            MultiSamples::Count1 => TextureUsage::STORAGE,
            _ => TextureUsage::empty(),
        };

        let color_target = Texture::new(
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
                    | TextureUsage::TRANSFER_SRC
                    | extra_usage,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("color_target".to_owned()),
            },
        )
        .unwrap();

        let depth_target = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::DEPTH_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                // NOTE: We need two array elements since we need a copy of depth to use
                // during the transparent pass.
                array_elements: 2,
                mip_levels: 1,
                sample_count: samples,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_SRC
                    | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("depth_target".to_owned()),
            },
        )
        .unwrap();

        let thin_g_target = Texture::new(
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
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED | extra_usage,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("thin_g_target".to_owned()),
            },
        )
        .unwrap();

        let vel_target = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::VEL_TARGET_FORMAT,
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
                debug_name: Some("vel_target".to_owned()),
            },
        )
        .unwrap();

        let norm_target = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::NORM_TARGET_FORMAT,
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
                debug_name: Some("norm_target".to_owned()),
            },
        )
        .unwrap();

        let linear_color = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::FINAL_COLOR_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 2,
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
        .unwrap();

        let entities = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Self::ENTITIES_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("entities_target".to_owned()),
            },
        )
        .unwrap();

        if samples == MultiSamples::Count1 {
            Attachments::SingleSample {
                color_target,
                depth_target,
                thin_g_target,
                vel_target,
                norm_target,
                linear_color,
                entities,
            }
        } else {
            Attachments::MultiSample {
                color_target,
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
                            | TextureUsage::STORAGE
                            | TextureUsage::TRANSFER_SRC,
                        memory_usage: MemoryUsage::GpuOnly,
                        queue_types: QueueTypes::MAIN,
                        sharing_mode: SharingMode::Exclusive,
                        debug_name: Some("color_resolve".to_owned()),
                    },
                )
                .unwrap(),
                depth_target,
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
                thin_g_target,
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
                            | TextureUsage::STORAGE,
                        memory_usage: MemoryUsage::GpuOnly,
                        queue_types: QueueTypes::MAIN,
                        sharing_mode: SharingMode::Exclusive,
                        debug_name: Some("thin_g_resolve".to_owned()),
                    },
                )
                .unwrap(),
                vel_target,
                vel_resolve: Texture::new(
                    ctx.clone(),
                    TextureCreateInfo {
                        format: Self::VEL_TARGET_FORMAT,
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
                        debug_name: Some("vel_resolve".to_owned()),
                    },
                )
                .unwrap(),
                norm_target,
                norm_resolve: Texture::new(
                    ctx.clone(),
                    TextureCreateInfo {
                        format: Self::NORM_TARGET_FORMAT,
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
                        debug_name: Some("norm_resolve".to_owned()),
                    },
                )
                .unwrap(),
                linear_color,
                entities,
            }
        }
    }
}
