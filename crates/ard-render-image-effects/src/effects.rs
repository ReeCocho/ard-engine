use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;

pub trait ImageEffect: Send {
    fn src_texture_type(&self) -> ImageEffectTextureType;

    fn dst_texture_type(&self) -> ImageEffectTextureType;

    fn bind_images(&mut self, frame: Frame, src: &Texture, dst: ImageEffectDst, depth: &Texture);

    fn render<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        dst: ImageEffectDst<'a>,
    );
}

pub struct ImageEffectTextures {
    pong_hdr: Texture,
    ping_sdr: Texture,
    pong_sdr: Texture,
}

pub struct ImageEffectsBindImages<'a> {
    textures: &'a ImageEffectTextures,
    effects: Vec<ImageEffectInstance<&'a mut dyn ImageEffect>>,
}

pub struct ImageEffectsRender<'a> {
    textures: &'a ImageEffectTextures,
    effects: Vec<ImageEffectInstance<&'a dyn ImageEffect>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageEffectTextureType {
    HDR,
    SDR,
}

pub enum ImageEffectDst<'a> {
    Offscreen(&'a Texture),
    Surface(&'a SurfaceImage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ImageEffectDstType {
    Surface,
    Offscreen,
}

struct ImageEffectInstance<R> {
    effect: R,
    /// Source image and a boolean for "ping-ponging."
    src: (ImageEffectTextureType, bool),
    /// Source image, a boolean for "ping-ponging," and the destination type.
    dst: (ImageEffectTextureType, bool, ImageEffectDstType),
}

impl ImageEffectTextures {
    pub fn new(ctx: &Context, hdr_format: Format, sdr_format: Format, dims: (u32, u32)) -> Self {
        ImageEffectTextures {
            pong_hdr: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: hdr_format,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("pong_hdr".into()),
                },
            )
            .unwrap(),
            ping_sdr: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: sdr_format,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("ping_sdr".into()),
                },
            )
            .unwrap(),
            pong_sdr: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: sdr_format,
                    ty: TextureType::Type2D,
                    width: dims.0,
                    height: dims.1,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("pong_sdr".into()),
                },
            )
            .unwrap(),
        }
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.pong_hdr = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: self.pong_hdr.format(),
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("pong_hdr".into()),
            },
        )
        .unwrap();

        self.ping_sdr = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: self.ping_sdr.format(),
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("ping_sdr".into()),
            },
        )
        .unwrap();

        self.pong_sdr = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: self.pong_sdr.format(),
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("pong_sdr".into()),
            },
        )
        .unwrap();
    }
}

impl<'a> ImageEffectsBindImages<'a> {
    pub fn new(textures: &'a ImageEffectTextures) -> Self {
        Self {
            textures,
            effects: Vec::default(),
        }
    }

    pub fn add(mut self, effect: &'a mut impl ImageEffect) -> Self {
        self.effects.push(ImageEffectInstance {
            src: (effect.src_texture_type(), self.effects.len() % 2 == 1),
            dst: (
                effect.dst_texture_type(),
                self.effects.len() % 2 == 0,
                ImageEffectDstType::Offscreen,
            ),
            effect,
        });

        self
    }

    pub fn bind(
        mut self,
        frame: Frame,
        color_src: &Texture,
        depth_src: &Texture,
        dst: &SurfaceImage,
    ) {
        if self.effects.is_empty() {
            return;
        }

        self.effects.last_mut().unwrap().dst.2 = ImageEffectDstType::Surface;

        self.effects.into_iter().for_each(|effect| {
            effect.effect.bind_images(
                frame,
                match effect.src {
                    (ImageEffectTextureType::HDR, false) => color_src,
                    (ImageEffectTextureType::HDR, true) => &self.textures.pong_hdr,
                    (ImageEffectTextureType::SDR, false) => &self.textures.ping_sdr,
                    (ImageEffectTextureType::SDR, true) => &self.textures.pong_sdr,
                },
                match effect.dst {
                    (ImageEffectTextureType::HDR, false, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(color_src)
                    }
                    (ImageEffectTextureType::HDR, true, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.pong_hdr)
                    }
                    (ImageEffectTextureType::SDR, false, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.ping_sdr)
                    }
                    (ImageEffectTextureType::SDR, true, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.pong_sdr)
                    }
                    (_, _, ImageEffectDstType::Surface) => ImageEffectDst::Surface(dst),
                },
                depth_src,
            );
        });
    }
}

impl<'a> ImageEffectsRender<'a> {
    pub fn new(textures: &'a ImageEffectTextures) -> Self {
        Self {
            textures,
            effects: Vec::default(),
        }
    }

    pub fn add(mut self, effect: &'a impl ImageEffect) -> Self {
        self.effects.push(ImageEffectInstance {
            effect,
            src: (effect.src_texture_type(), self.effects.len() % 2 == 1),
            dst: (
                effect.dst_texture_type(),
                self.effects.len() % 2 == 0,
                ImageEffectDstType::Offscreen,
            ),
        });

        self
    }

    pub fn render(
        mut self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        color_src: &'a Texture,
        dst: &'a SurfaceImage,
    ) {
        if self.effects.is_empty() {
            return;
        }

        self.effects.last_mut().unwrap().dst.2 = ImageEffectDstType::Surface;

        self.effects.into_iter().for_each(|effect| {
            effect.effect.render(
                frame,
                commands,
                match effect.dst {
                    (ImageEffectTextureType::HDR, false, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(color_src)
                    }
                    (ImageEffectTextureType::HDR, true, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.pong_hdr)
                    }
                    (ImageEffectTextureType::SDR, false, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.ping_sdr)
                    }
                    (ImageEffectTextureType::SDR, true, ImageEffectDstType::Offscreen) => {
                        ImageEffectDst::Offscreen(&self.textures.pong_sdr)
                    }
                    (_, _, ImageEffectDstType::Surface) => ImageEffectDst::Surface(dst),
                },
            );
        });
    }
}

impl<'a> ImageEffectDst<'a> {
    pub fn into_attachment_source(&self) -> ColorAttachmentSource<'a, ard_pal::Backend> {
        match self {
            ImageEffectDst::Surface(image) => ColorAttachmentSource::SurfaceImage(image),
            ImageEffectDst::Offscreen(image) => ColorAttachmentSource::Texture {
                texture: image,
                array_element: 0,
                mip_level: 0,
            },
        }
    }
}
