pub mod components;
pub mod systems;

use ard_core::prelude::*;
use systems::{destroy::Destroyer, stat::MarkStaticSystem, transform::TransformUpdate};

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(Destroyer::default());
        app.add_system(TransformUpdate::default());
        app.add_system(MarkStaticSystem::default());
    }
}
