use std::time::Duration;

use crate::{camera::CameraDescriptor, prelude::Backend, AnisotropyLevel};
use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec3};

/// Event indicating that rendering is about to be performed.
#[derive(Debug, Event, Copy, Clone)]
pub struct PreRender;

/// Event indicating that rendering has finished.
#[derive(Debug, Event, Copy, Clone)]
pub struct PostRender;

/// Represents an object to be rendered. When paired with a `Model` component, the `Renderer`
/// will present it to the screen dynamically.
#[derive(Clone)]
pub struct Renderable<B: Backend> {
    pub mesh: B::Mesh,
    pub material: B::Material,
}

impl<B: Backend> Component for Renderable<B> {}

/// Model matrix for a `Renderable` component.
#[derive(Component, Clone)]
pub struct Model(pub Mat4);

/// Handle to a static renderable object.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticRenderable(u32);

/// Renderer system settings.
#[derive(Debug, Resource)]
pub struct RendererSettings {
    /// Time between frame draws. `None` indicates no render limiting.
    pub render_time: Option<Duration>,
    /// Width and height of the renderer image. `None` indicates the dimensions should match that
    /// of the surface being presented to.
    pub canvas_size: Option<(u32, u32)>,
    /// Anisotropy level to be used for textures. Can be `None` for no filtering.
    pub anisotropy_level: Option<AnisotropyLevel>,
}

pub struct RendererCreateInfo<'a, B: Backend> {
    pub ctx: &'a B::GraphicsContext,
    pub surface: &'a B::Surface,
    pub settings: &'a RendererSettings,
}

/// Primary game renderer.
///
/// This system draws to the `Surface` in `Resources`.
pub trait RendererApi<B: Backend>: SystemState + Sized {
    /// Creates the renderer and the associated factory for creating rendering objects.
    fn new(
        create_info: &RendererCreateInfo<B>,
    ) -> (Self, B::Factory, B::StaticGeometry, B::DebugDrawing);
}

/// Used to register objects that don't move when rendering.
///
/// ## Note About Performance
/// In some implementations (like Vulkan), it is VERY bad for performance to be registering and
/// unregistering objects. You should prefer registering and unregistering static objects in bulk
/// and infrequently (like once when loading a map).
pub trait StaticGeometryApi<B: Backend>: Resource + Send + Sync {
    /// Registers renderables as static. Will write output handles to the `handles` argument in the
    /// same order as the renderable. If `handles.len() < renderables.len()`, then not all handles
    /// will be written to the buffer.
    ///
    /// ## Note
    /// If you drop a static renderable handle, it is impossible thereafter to unregister the
    /// renderable.
    fn register(&self, models: &[(Renderable<B>, Model)], handles: &mut [StaticRenderable]);

    fn unregister(&self, handles: &[StaticRenderable]);
}

/// Used to draw simple debugging shapes like lines, prisims, and spheres. All debug shapes that
/// are registered are reset at the end of `PostRender`.
pub trait DebugDrawingApi<B: Backend>: Resource + Send + Sync {
    /// Draw a line from one point in world space to another.
    fn draw_line(&self, a: Vec3, b: Vec3, color: Vec3);

    /// Draw a sphere with the given center and radius.
    fn draw_sphere(&self, center: Vec3, radius: f32, color: Vec3);

    /// Draw a camera frustum.
    fn draw_frustum(&self, descriptor: CameraDescriptor, color: Vec3);

    /// Draw a rectangular prism.
    fn draw_rect_prism(&self, half_extents: Vec3, transform: Mat4, color: Vec3);
}

impl Default for RendererSettings {
    fn default() -> Self {
        RendererSettings {
            render_time: Some(Duration::from_secs_f32(1.0 / 60.0)),
            canvas_size: None,
            anisotropy_level: None,
        }
    }
}

impl StaticRenderable {
    #[inline]
    pub fn new(id: u32) -> Self {
        StaticRenderable(id)
    }

    #[inline]
    pub fn empty() -> Self {
        StaticRenderable(0)
    }
}

impl From<u32> for StaticRenderable {
    #[inline]
    fn from(id: u32) -> Self {
        StaticRenderable::new(id)
    }
}
