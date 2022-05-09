use thiserror::*;

use ard_ecs::prelude::*;
use ard_window::prelude::WindowId;

use crate::prelude::*;

#[derive(Debug)]
pub struct GraphicsContextCreateInfo {
    /// Window to create the initial surface with.
    pub window: WindowId,
    /// Enable backend specific debugging messages.
    pub debug: bool,
}

/// Error type when creating a graphics context. Since different backends have different kinds
/// of errors, a human readable string error is returned.
#[derive(Error, Debug)]
#[error("graphics context creation failed with error `{0}`")]
pub struct GraphicsContextCreateError(pub String);

/// The graphics context is the interface between the engine and the graphics hardware on the
/// system. It is used by things like the renderer to actually perform graphics operations.
pub trait GraphicsContextApi<B: Backend>: Clone + Sized + Resource + Send + Sync {
    /// Creates a new graphics context and the surface for the provided window.
    fn new(
        resources: &Resources,
        create_info: &GraphicsContextCreateInfo,
    ) -> Result<(Self, B::Surface), GraphicsContextCreateError>;
}

impl Default for GraphicsContextCreateInfo {
    fn default() -> Self {
        Self {
            window: WindowId::primary(),
            debug: false,
        }
    }
}
