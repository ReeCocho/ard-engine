pub mod components;
pub mod destroy;
pub mod lighting;
pub mod object;
pub mod scene;
pub mod serialization;
pub mod systems;

use ard_core::prelude::*;
use destroy::Destroyer;
use object::{
    empty::{EmptyObject, EmptyObjectPack},
    static_object::{StaticObject, StaticObjectPack},
};
use serde::{Deserialize, Serialize};
use systems::{renderable::ApplyRenderableData, transform::TransformUpdate};

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct GamePlugin;

// Scene definition.
scene_definition! {
    Scene,
    StaticObject
    EmptyObject
}

impl Plugin for GamePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(Destroyer::default());
        app.add_system(TransformUpdate::default());
        app.add_system(ApplyRenderableData);
    }
}
