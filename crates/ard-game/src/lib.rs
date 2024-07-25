pub mod components;
pub mod save_data;
pub mod systems;

use ard_assets::prelude::Assets;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use save_data::{SceneAsset, SceneLoader};
use systems::{running::GameRunningSystem, stat::MarkStaticSystem};

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct GamePlugin;

#[derive(Resource, Clone, Copy)]
pub struct GameRunning(pub bool);

#[derive(Event, Clone, Copy)]
pub struct GameStart;

#[derive(Event, Clone, Copy)]
pub struct GameStop;

impl Plugin for GamePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(MarkStaticSystem::default());
        app.add_system(GameRunningSystem);
        app.add_resource(GameRunning(false));
        app.add_startup_function(startup);
    }
}

fn startup(app: &mut App) {
    let assets = app.resources.get::<Assets>().unwrap();
    assets.register::<SceneAsset>(SceneLoader);
}
