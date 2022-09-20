use std::sync::Arc;

use crate::{context::Context, Backend};
use thiserror::Error;

pub struct ShaderCreateInfo<'a> {
    /// Shader code for the module. The format depends on which backend is being used.
    pub code: &'a [u8],
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum ShaderCreateError {
    #[error("an error occured: {0}")]
    Other(String),
}

pub struct Shader<B: Backend>(pub(crate) Arc<ShaderInner<B>>);

pub(crate) struct ShaderInner<B: Backend> {
    ctx: Context<B>,
    pub(crate) id: B::Shader,
}

impl<B: Backend> Shader<B> {
    /// Creates a new shader.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `create_info` - Describes the shader to create.
    #[inline(always)]
    pub fn new(
        ctx: Context<B>,
        create_info: ShaderCreateInfo<'_>,
    ) -> Result<Self, ShaderCreateError> {
        let id = unsafe { ctx.0.create_shader(create_info)? };
        Ok(Shader(Arc::new(ShaderInner { ctx, id })))
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::Shader {
        &self.0.id
    }
}

impl<B: Backend> Drop for ShaderInner<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_shader(&mut self.id);
        }
    }
}

impl<B: Backend> Clone for Shader<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
