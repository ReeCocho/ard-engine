use ard_ecs::prelude::*;
use ard_physics::engine::PhysicsEngine;

use crate::{GameRunning, GameStart, GameStop};

#[derive(SystemState)]
pub struct GameRunningSystem;

impl GameRunningSystem {
    fn game_start(
        &mut self,
        _: GameStart,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<GameRunning>, Write<PhysicsEngine>)>,
    ) {
        res.get_mut::<GameRunning>().unwrap().0 = true;
        res.get_mut::<PhysicsEngine>()
            .unwrap()
            .set_simulation_enabled(true);
    }

    fn game_stop(
        &mut self,
        _: GameStop,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<GameRunning>, Write<PhysicsEngine>)>,
    ) {
        res.get_mut::<GameRunning>().unwrap().0 = false;
        res.get_mut::<PhysicsEngine>()
            .unwrap()
            .set_simulation_enabled(false);
    }
}

impl From<GameRunningSystem> for System {
    fn from(value: GameRunningSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(GameRunningSystem::game_start)
            .with_handler(GameRunningSystem::game_stop)
            .build()
    }
}
