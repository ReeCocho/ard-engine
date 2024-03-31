use ard_ecs::component::Component;

pub mod ecs;
pub mod module;
pub mod resource;

/// Describes what type of rendering is required for a particular entity.
#[derive(Component, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderingMode {
    /// The entity is fully opaque.
    Opaque = 0,
    /// The entity is opaque, but might have holes in the geometry from alpha masking.
    AlphaCutout = 1,
    /// The entity is transparent,
    Transparent = 2,
}
