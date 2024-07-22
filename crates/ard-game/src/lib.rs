pub mod components;
pub mod systems;

use ard_core::prelude::*;
use systems::stat::MarkStaticSystem;

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(MarkStaticSystem::default());
    }
}
