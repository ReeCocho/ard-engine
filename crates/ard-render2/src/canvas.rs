use ard_ecs::prelude::*;
use ard_pal::prelude::*;
use ard_render_camera::target::RenderTarget;
use ard_render_renderers::highz::{HzbImage, HzbRenderer};

use crate::FRAMES_IN_FLIGHT;

#[derive(Resource)]
pub(crate) struct Canvas {
    /// The render target to draw to for the canvas.
    render_target: RenderTarget,
    /// HZB image for occlusion culling.
    hzb: HzbImage<FRAMES_IN_FLIGHT>,
    /// Surface being rendered to.
    surface: Surface,
    /// Surface image for the current frame.
    image: Option<SurfaceImage>,
    /// Size of the canvas.
    size: (u32, u32),
    /// Presentation mode being used.
    present_mode: PresentMode,
    /// Surface image format.
    format: Format,
}

impl Canvas {
    pub fn new(
        ctx: &Context,
        surface: Surface,
        dims: (u32, u32),
        present_mode: PresentMode,
        hzb_render: &HzbRenderer,
    ) -> Self {
        let mut canvas = Self {
            image: None,
            render_target: RenderTarget::new(ctx, dims),
            hzb: HzbImage::new(hzb_render, dims.0, dims.1),
            size: dims,
            surface,
            present_mode,
            format: Format::Bgra8Unorm,
        };
        canvas.bind_hzb_src();
        canvas
    }

    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    #[inline(always)]
    pub fn render_target(&self) -> &RenderTarget {
        &self.render_target
    }

    #[inline(always)]
    pub fn hzb(&self) -> &HzbImage<FRAMES_IN_FLIGHT> {
        &self.hzb
    }

    pub fn blit_to_surface<'a>(&'a self, commands: &mut CommandBuffer<'a>) {
        let (width, height) = self.surface.dimensions();
        let color = self.render_target.color();
        commands.blit(
            BlitSource::Texture(color),
            BlitDestination::SurfaceImage(self.image()),
            Blit {
                src_min: (0, 0, 0),
                src_max: color.dims(),
                src_mip: 0,
                src_array_element: 0,
                dst_min: (0, 0, 0),
                dst_max: (width, height, 1),
                dst_mip: 0,
                dst_array_element: 0,
            },
            Filter::Linear,
        );
    }

    pub fn acquire_image(&mut self) {
        self.image = Some(self.surface.acquire_image().unwrap());
    }

    /// Gets the current surface image.
    ///
    /// # Note
    /// This will panic if `image` is `None`. This is really just for convenience since it should
    /// never be `None` when the render ECS is executing.
    pub fn image(&self) -> &SurfaceImage {
        self.image.as_ref().unwrap()
    }

    /// Updates the canvas with a new size. Does nothing if the size is matching.
    pub fn resize(&mut self, ctx: &Context, hzb_render: &HzbRenderer, dims: (u32, u32)) {
        if dims == self.size {
            return;
        }

        self.size = dims;
        self.render_target = RenderTarget::new(ctx, dims);
        self.hzb = HzbImage::new(hzb_render, dims.0, dims.1);
        self.bind_hzb_src();
    }

    /// Presents the currently active surface image and optionally resizes the surface to meet the
    /// window size if needed.
    pub fn present(&mut self, ctx: &Context, window_size: (u32, u32)) {
        let image = match self.image.take() {
            Some(image) => image,
            None => return,
        };

        if let SurfacePresentSuccess::Invalidated =
            ctx.present().present(&self.surface, image).unwrap()
        {
            let present_mode = self.present_mode;
            let format = self.format;
            self.surface
                .update_config(SurfaceConfiguration {
                    width: window_size.0,
                    height: window_size.1,
                    present_mode,
                    format,
                })
                .unwrap();
        }
    }

    fn bind_hzb_src(&mut self) {
        for i in 0..FRAMES_IN_FLIGHT {
            self.hzb.bind_src(i.into(), self.render_target.depth());
        }
    }
}
