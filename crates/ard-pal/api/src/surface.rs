use std::fmt::Debug;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use thiserror::Error;

use crate::{
    context::Context,
    types::{Format, PresentMode},
    Backend,
};

pub struct SurfaceCreateInfo<W: HasWindowHandle + HasDisplayHandle> {
    /// Default surface configuration.
    pub config: SurfaceConfiguration,
    /// Raw window handle used by whatever windowing API you choose.
    pub window: WindowSource<W>,
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

pub enum WindowSource<W: HasWindowHandle + HasDisplayHandle> {
    Raw {
        window: RawWindowHandle,
        display: RawDisplayHandle,
    },
    Reference(W),
}

pub struct SurfaceConfiguration {
    /// Width in pixels of the surface.
    pub width: u32,
    /// Height in pixels of the surface.
    pub height: u32,
    /// Preferred presentation mode of the surface.
    pub present_mode: PresentMode,
    /// Preferred texture format of the surface.
    pub format: Format,
}

pub struct SurfaceCapabilities {
    /// Minimum surface width and height.
    pub min_size: (u32, u32),
    /// Maximum surface width and height.
    pub max_size: (u32, u32),
    /// Available presentation modes.
    pub present_modes: Vec<PresentMode>,
}

pub struct Surface<B: Backend> {
    ctx: Context<B>,
    dims: (u32, u32),
    pub(crate) id: B::Surface,
}

pub struct SurfaceImage<B: Backend> {
    ctx: Context<B>,
    pub(crate) id: B::SurfaceImage,
}

#[derive(Error, Debug)]
pub enum SurfaceCreateError {
    #[error("bad surface configuration: `{0}`")]
    BadConfig(SurfaceUpdateError),
    #[error("a error has occured: `{0}`")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum SurfaceUpdateError {
    #[error("at least one image is still pending presentation")]
    ImagePending,
    #[error("a error has occured: `{0}`")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum SurfaceImageAcquireError {
    #[error("no available images")]
    NoImages,
    #[error("a error has occured: `{0}`")]
    Other(String),
}

#[derive(Error)]
pub enum SurfacePresentError<B: Backend> {
    #[error("the image was not drawn to before presenting")]
    NoRender(SurfaceImage<B>),
    #[error("image did not come from this surface")]
    BadImage(SurfaceImage<B>),
    #[error("a error has occured: `{0}`")]
    Other(String),
}

pub enum SurfacePresentSuccess {
    /// Surface presentation succeeded.
    Ok,
    /// Surface presentation succeeded, but the surface is invalidated and needs a new config,
    Invalidated,
}

impl<B: Backend> Surface<B> {
    pub fn new<W: HasWindowHandle + HasDisplayHandle>(
        ctx: Context<B>,
        create_info: SurfaceCreateInfo<W>,
    ) -> Result<Self, SurfaceCreateError> {
        let dims = (create_info.config.width, create_info.config.height);
        let id = unsafe { ctx.0.create_surface(create_info)? };
        Ok(Self { ctx, id, dims })
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::Surface {
        &self.id
    }

    #[inline(always)]
    pub fn dimensions(&self) -> (u32, u32) {
        self.dims
    }

    /// Gets most up to date surface capabilities.
    #[inline(always)]
    pub fn get_capabilities(&self) -> SurfaceCapabilities {
        unsafe { self.ctx.0.get_surface_capabilities(&self.id) }
    }

    /// Update the configuration of the surface.
    ///
    /// There must not be any images pending presentation before the configuration is updated.
    #[inline(always)]
    pub fn update_config(
        &mut self,
        config: SurfaceConfiguration,
    ) -> Result<(), SurfaceUpdateError> {
        unsafe {
            self.dims = self.ctx.0.update_surface(&mut self.id, config)?;
        };
        Ok(())
    }

    /// Acquire a new image from the surface to present.
    #[inline(always)]
    pub fn acquire_image(&mut self) -> Result<SurfaceImage<B>, SurfaceImageAcquireError> {
        let id = unsafe { self.ctx.0.acquire_image(&mut self.id)? };
        Ok(SurfaceImage {
            ctx: self.ctx.clone(),
            id,
        })
    }
}

impl<B: Backend> SurfaceImage<B> {
    #[inline(always)]
    pub fn internal(&self) -> &B::SurfaceImage {
        &self.id
    }
}

impl<B: Backend> Drop for Surface<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_surface(&mut self.id);
        }
    }
}

impl<B: Backend> Drop for SurfaceImage<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_surface_image(&mut self.id);
        }
    }
}

impl<B: Backend> Debug for SurfaceImage<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceImage").finish()
    }
}

impl<B: Backend> Debug for SurfacePresentError<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoRender(arg0) => f.debug_tuple("NoRender").field(arg0).finish(),
            Self::BadImage(arg0) => f.debug_tuple("BadImage").field(arg0).finish(),
            Self::Other(arg0) => f.debug_tuple("Other").field(arg0).finish(),
        }
    }
}
