mod alloc;
pub mod camera;
pub mod context;
pub mod cube_map;
pub mod debug_gui;
pub mod factory;
pub mod lighting;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod renderer;
pub mod shader;
pub mod shader_constants;
pub mod surface;
pub mod texture;
pub mod util;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use prelude::static_geometry::StaticGeometry;
use prelude::*;

pub mod prelude {
    pub use crate::camera::*;
    pub use crate::context::*;
    pub use crate::cube_map::*;
    pub use crate::debug_gui::*;
    pub use crate::factory::*;
    pub use crate::lighting::*;
    pub use crate::material::*;
    pub use crate::mesh::*;
    pub use crate::pipeline::*;
    pub use crate::renderer::debug_drawing::*;
    pub use crate::renderer::entity_image::*;
    pub use crate::renderer::static_geometry::*;
    pub use crate::renderer::*;
    pub use crate::shader::*;
    pub use crate::surface::*;
    pub use crate::texture::*;
    pub use crate::*;
    pub use ard_graphics_api::prelude::*;
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct VkBackend;

impl Backend for VkBackend {
    type GraphicsContext = GraphicsContext;
    type Surface = Surface;
    type Renderer = Renderer;
    type Factory = Factory;
    type Lighting = Lighting;
    type Mesh = Mesh;
    type Shader = Shader;
    type Pipeline = Pipeline;
    type Material = Material;
    type Camera = Camera;
    type StaticGeometry = StaticGeometry;
    type DebugDrawing = DebugDrawing;
    type EntityImageApi = EntityImage;
    type Texture = Texture;
    type DebugGui = DebugGui;
    type CubeMap = CubeMap;

    const MAX_MESHES: usize = 2048;
    const MAX_SHADERS: usize = 4096;
    const MAX_PIPELINES: usize = 1024;
    const MAX_MATERIALS: usize = 2048;
    const MAX_CAMERA: usize = 64;
    const MAX_TEXTURES: usize = 2048;
    const MAX_CUBE_MAPS: usize = 128;
    const MAX_TEXTURES_PER_MATERIAL: usize = shader_constants::MAX_TEXTURES_PER_MATERIAL;
}

pub struct VkGraphicsPlugin {
    pub context_create_info: GraphicsContextCreateInfo,
}

#[derive(Resource)]
struct LateGraphicsContextCreateInfo(GraphicsContextCreateInfo);

impl Plugin for VkGraphicsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        let create_info = std::mem::take(&mut self.context_create_info);

        app.add_resource(LateGraphicsContextCreateInfo(create_info));
        app.add_startup_function(late_context_creation);
    }
}

fn late_context_creation(app: &mut App) {
    let create_info = app
        .resources
        .get::<LateGraphicsContextCreateInfo>()
        .unwrap();

    let (ctx, surface) = GraphicsContext::new(&app.resources, &create_info.0).unwrap();
    let renderer_settings = RendererSettings::default();
    let (renderer, factory, static_geo, debug_drawing, debug_gui, lighting) =
        Renderer::new(&RendererCreateInfo {
            ctx: &ctx,
            surface: &surface,
            settings: &renderer_settings,
        });

    let mut entity_image = EntityImage::default();
    entity_image.resize(match renderer_settings.canvas_size {
        Some(dims) => ash::vk::Extent2D {
            width: dims.0,
            height: dims.1,
        },
        None => surface.0.lock().unwrap().resolution,
    });

    app.resources.add(entity_image);
    app.resources.add(static_geo);
    app.resources.add(factory);
    app.resources.add(renderer_settings);
    app.resources.add(surface);
    app.resources.add(debug_drawing);
    app.resources.add(debug_gui);
    app.resources.add(lighting);
    app.dispatcher.add_system(renderer);
    app.resources.add(ctx);
}
