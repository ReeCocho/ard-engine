pub mod components;
pub mod save_data;
pub mod settings;
pub mod systems;

use std::time::Duration;

use ard_assets::prelude::Assets;
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_render::{MsaaSettings, PresentationSettings};
use ard_render_gui::Gui;
use ard_render_image_effects::smaa::SmaaSettings;
use save_data::{InitialSceneAsset, InitialSceneLoader, SceneAsset, SceneLoader};
use settings::GameSettings;
use systems::{
    actor::ActorMoveSystem,
    pause::{PauseGui, PauseSystem},
    player::{PlayerInputSystem, PlayerSpawnSystem},
    running::GameRunningSystem,
    stat::MarkStaticSystem,
};

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct GamePlugin;

#[derive(Resource, Clone, Copy)]
pub struct GameRunning(pub bool);

#[derive(Resource)]
pub struct IsEditor;

#[derive(Event, Clone, Copy)]
pub struct GameStart;

#[derive(Event, Clone, Copy)]
pub struct GameStop;

impl Plugin for GamePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_system(MarkStaticSystem::default());
        app.add_system(GameRunningSystem);
        app.add_system(ActorMoveSystem);
        app.add_system(PlayerSpawnSystem);
        app.add_system(PlayerInputSystem::default());
        app.add_system(PauseSystem);
        app.add_resource(GameRunning(false));
        app.add_startup_function(startup);
    }
}

fn startup(app: &mut App) {
    let assets = app.resources.get::<Assets>().unwrap();
    assets.register::<SceneAsset>(SceneLoader);
    assets.register::<InitialSceneAsset>(InitialSceneLoader);

    let settings = app.resources.get::<GameSettings>().unwrap();
    app.resources.get_mut::<SmaaSettings>().unwrap().enabled = settings.smaa;
    app.resources.get_mut::<MsaaSettings>().unwrap().samples = settings.msaa;
    app.resources
        .get_mut::<PresentationSettings>()
        .unwrap()
        .render_time = settings
        .target_frame_rate
        .map(|v| Duration::from_secs_f32(1.0 / v.max(30) as f32));

    if app.resources.get::<IsEditor>().is_none() {
        app.resources
            .get_mut::<Gui>()
            .unwrap()
            .add_view(PauseGui::default());
    }
}
