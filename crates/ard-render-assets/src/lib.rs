use ard_assets::prelude::Assets;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render::factory::Factory;
use material::{MaterialAsset, MaterialLoader};
use mesh::{MeshAsset, MeshLoader};
use model::{ModelAsset, ModelLoader};
use texture::{TextureAsset, TextureLoader};

pub mod material;
pub mod mesh;
pub mod model;
pub mod texture;

#[derive(Resource, Clone)]
pub struct RenderAssetsPlugin;

impl Plugin for RenderAssetsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_startup_function(late_init);
    }
}

fn late_init(app: &mut App) {
    let assets = app.resources.get::<Assets>().unwrap();
    let factory = app.resources.get::<Factory>().unwrap();
    assets.register::<ModelAsset>(ModelLoader);
    assets.register::<TextureAsset>(TextureLoader::new(factory.clone()));
    assets.register::<MeshAsset>(MeshLoader::new(factory.clone()));
    assets.register::<MaterialAsset>(MaterialLoader::new(factory.clone()));
}
