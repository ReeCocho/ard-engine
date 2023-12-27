use ard_assets::prelude::Assets;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render2::factory::Factory;
use model::{ModelAsset, ModelLoader};

pub mod model;

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
    assets.register::<ModelAsset>(ModelLoader::new(factory.clone()));
}
