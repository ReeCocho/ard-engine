pub mod cube_map;
pub mod material;
pub mod model;
pub mod texture;

use ard_assets::prelude::*;
use ard_core::prelude::*;
use ard_ecs::prelude::*;

use crate::factory::Factory;

use self::{
    cube_map::{CubeMapAsset, CubeMapLoader},
    material::{MaterialAsset, MaterialLoader},
    model::{ModelAsset, ModelLoader},
};

#[derive(Resource, Clone)]
pub struct RenderAssetsPlugin {
    pub pbr_material: AssetNameBuf,
}

impl Plugin for RenderAssetsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(self.clone());
        app.add_startup_function(post_init);
    }
}

fn post_init(app: &mut App) {
    let plugin = app.resources.get::<RenderAssetsPlugin>().unwrap().clone();
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();

    // Register loaders
    assets.register::<MaterialAsset>(MaterialLoader::new(factory.clone()));
    assets.register::<CubeMapAsset>(CubeMapLoader::new(factory.clone()));

    // Load in required materials
    let handle = assets.load::<MaterialAsset>(&plugin.pbr_material);
    assets.wait_for_load(&handle);

    let pbr_material = assets.get(&handle).unwrap();

    // Register model loader
    assets.register::<ModelAsset>(ModelLoader::new(
        factory.clone(),
        pbr_material.material.clone(),
    ));
}
