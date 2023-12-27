use ard_ecs::prelude::*;
use ard_pal::prelude::*;

use crate::CameraClearColor;

#[derive(Component)]
pub struct RenderTarget {
    ctx: Context,
    dims: (u32, u32),
    color: Texture,
    depth: Texture,
}

impl RenderTarget {
    pub const COLOR_FORMAT: Format = Format::Rgba16SFloat;
    pub const DEPTH_FORMAT: Format = Format::D32Sfloat;

    pub fn new(ctx: &Context, dims: (u32, u32)) -> Self {
        debug_assert!(dims.0 > 0 && dims.1 > 0);

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
                texture_usage: TextureUsage::COLOR_ATTACHMENT
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_SRC,
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
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("depth_target".to_owned()),
            },
        )
        .unwrap();

        Self {
            ctx: ctx.clone(),
            color,
            depth,
            dims,
        }
    }

    pub fn hzb_pass(&self) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: &self.depth,
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Clear(ClearColor::D32S32(0.0, 0)),
                store_op: StoreOp::Store,
            }),
        }
    }

    pub fn depth_prepass(&self) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: &self.depth,
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
            }),
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
            }],
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: &self.depth,
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
            }),
        }
    }

    pub fn transparent_pass(&self) -> RenderPassDescriptor {
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::Texture {
                    texture: &self.color,
                    array_element: 0,
                    mip_level: 0,
                },
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: Some(DepthStencilAttachment {
                texture: &self.depth,
                array_element: 0,
                mip_level: 0,
                load_op: LoadOp::Load,
                store_op: StoreOp::DontCare,
            }),
        }
    }

    #[inline(always)]
    pub fn dims(&self) -> (u32, u32) {
        self.dims
    }

    #[inline(always)]
    pub fn color(&self) -> &Texture {
        &self.color
    }

    #[inline(always)]
    pub fn depth(&self) -> &Texture {
        &self.depth
    }

    pub fn resize(&mut self, dims: (u32, u32)) {
        debug_assert!(dims.0 > 0 && dims.1 > 0);

        self.color = Texture::new(
            self.ctx.clone(),
            TextureCreateInfo {
                format: Self::COLOR_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("color_target".to_owned()),
            },
        )
        .unwrap();

        self.depth = Texture::new(
            self.ctx.clone(),
            TextureCreateInfo {
                format: Self::DEPTH_FORMAT,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("depth_target".to_owned()),
            },
        )
        .unwrap();

        self.dims = dims;
    }
}
