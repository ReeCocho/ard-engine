use std::time::Duration;

use crate::{camera::CameraDescriptor, prelude::Backend, AnisotropyLevel, RenderLayerFlags};
use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Event indicating that rendering is about to be performed. Contains the duration sine the
/// last pre render event.
#[derive(Debug, Event, Copy, Clone)]
pub struct PreRender(pub Duration);

/// Event indicating that rendering has finished. Contains the duration since the
/// last post render event.
#[derive(Debug, Event, Copy, Clone)]
pub struct PostRender(pub Duration);

/// Event indicating an entity image has rendered and is available for sampling.
#[derive(Debug, Event, Copy, Clone)]
pub struct NewEntityImage;

/// User submitable event that indicates the next frame should render an entity image.
#[derive(Debug, Event, Copy, Clone)]
pub struct RenderEntityImage;

/// Represents an object to be rendered. When paired with a `Model` component, the `Renderer`
/// will present it to the screen dynamically.
#[derive(Clone)]
pub struct Renderable<B: Backend> {
    pub mesh: B::Mesh,
    pub material: B::Material,
    pub layers: RenderLayerFlags,
}

/// An object to be rendered that will not move.
pub struct StaticRenderable<B: Backend> {
    /// Describes how the object looks.
    pub renderable: Renderable<B>,
    /// Transformation of the object.
    pub model: Model,
    /// Entity associated with the object.
    pub entity: Entity,
}

impl<B: Backend> Component for Renderable<B> {}

/// Model matrix for a `Renderable` component.
#[derive(Component, Default, Copy, Clone, Serialize, Deserialize)]
pub struct Model(pub Mat4);

/// Handle to a static renderable object.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticRenderableHandle(u32);

/// Renderer system settings.
#[derive(Debug, Resource)]
pub struct RendererSettings {
    /// Flag to enable drawing the game scene. For games, this should be `true` all the time. This
    /// is useful for things like editors where you only want a GUI.
    pub render_scene: bool,
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
    ) -> (
        Self,
        B::Factory,
        B::StaticGeometry,
        B::DebugDrawing,
        B::DebugGui,
        B::Lighting,
    );
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
    fn register(&self, models: &[StaticRenderable<B>], handles: &mut [StaticRenderableHandle]);

    fn unregister(&self, handles: &[StaticRenderableHandle]);
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

/// Contains an image of the scene where each pixel is an entity. Submit the `RenderEntityImage`
/// command in order to get this image rendered.
///
/// # Note
/// The rendered image may be at a lower resolution than the actual canvas size.
pub trait EntityImageApi<B: Backend>: Resource + Send + Sync {
    /// Sample an entity from the image using UV coordinates.
    fn sample(&self, uv: Vec2) -> Option<Entity>;
}

impl Default for RendererSettings {
    fn default() -> Self {
        RendererSettings {
            render_scene: true,
            render_time: Some(Duration::from_secs_f32(1.0 / 60.0)),
            canvas_size: None,
            anisotropy_level: None,
        }
    }
}

impl StaticRenderableHandle {
    #[inline]
    pub fn new(id: u32) -> Self {
        StaticRenderableHandle(id)
    }

    #[inline]
    pub fn empty() -> Self {
        StaticRenderableHandle(0)
    }
}

impl From<u32> for StaticRenderableHandle {
    #[inline]
    fn from(id: u32) -> Self {
        StaticRenderableHandle::new(id)
    }
}
