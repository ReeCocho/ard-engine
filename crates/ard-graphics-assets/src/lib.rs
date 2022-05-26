use ard_assets::prelude::*;
use ard_core::prelude::*;
use ard_graphics_vk::prelude as graphics;

pub mod model;
pub mod pbr;
pub mod pipelines;
pub mod shaders;
pub mod textures;

pub mod prelude {
    pub use crate::{model::*, pbr::*, pipelines::*, shaders::*, textures::*, *};
}

use prelude::*;

/// Plugin to support graphical assets.
pub struct GraphicsAssetsPlugin;

impl Plugin for GraphicsAssetsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_startup_function(late_loader_register);
    }
}

// Required because the graphics context isn't created until later.
fn late_loader_register(app: &mut App) {
    let factory = app
        .resources
        .get::<graphics::Factory>()
        .expect("graphics plugin required")
        .clone();

    let assets = app
        .resources
        .get::<Assets>()
        .expect("assets plugin required");

    assets.register::<TextureAsset>(TextureLoader {
        factory: factory.clone(),
    });
    assets.register::<PipelineAsset>(PipelineLoader {
        factory: factory.clone(),
    });
    assets.register::<ShaderAsset>(ShaderLoader {
        factory: factory.clone(),
    });
    assets.register::<PbrMaterialAsset>(PbrMaterialLoader {
        factory: factory.clone(),
    });
    assets.register::<ModelAsset>(ModelLoader { factory });
}
