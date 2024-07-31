use std::time::Duration;

use ard_core::core::Tick;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_math::{EulerRot, Quat, Vec3};
use ard_transform::{Children, Model, Rotation};

use crate::{
    components::{
        actor::Actor,
        player::{Player, PlayerCamera, PlayerSpawn},
    },
    GameRunning, GameStart,
};

use super::actor::ActorMoveSystem;

#[derive(SystemState)]
pub struct PlayerInputSystem {}

#[derive(SystemState)]
pub struct PlayerSpawnSystem;

impl Default for PlayerInputSystem {
    fn default() -> Self {
        Self {}
    }
}

impl PlayerInputSystem {
    pub fn on_tick(
        &mut self,
        tick: Tick,
        _: Commands,
        queries: Queries<(
            Write<Actor>,
            Write<Player>,
            Write<Rotation>,
            Read<Children>,
            Read<Model>,
        )>,
        res: Res<(Read<InputState>, Read<GameRunning>)>,
    ) {
        if !res.get::<GameRunning>().unwrap().0 {
            return;
        }

        let dt = tick.0.as_secs_f32();
        let input = res.get::<InputState>().unwrap();

        let (xdel, ydel) = input.mouse_delta();

        // Move the player cameras
        for rotation in queries
            .filter()
            .with::<PlayerCamera>()
            .make::<Write<Rotation>>()
        {
            let (mut ry, mut rx, rz) = rotation.0.to_euler(EulerRot::YXZ);

            rx += ydel as f32 * 0.003;
            ry += xdel as f32 * 0.003;
            rx = rx.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.05,
                std::f32::consts::FRAC_PI_2 - 0.05,
            );

            rotation.0 = Quat::from_euler(EulerRot::YXZ, ry, rx, rz);
        }

        // Apply movement to the players
        for (player, children, actor) in
            queries
                .filter()
                .make::<(Write<Player>, Read<Children>, Write<Actor>)>()
        {
            // Determine forward vector from the camera
            let camera = children.0[0];
            let camera_model = queries.get::<Read<Model>>(camera).unwrap();

            // Movement
            let mut del = Vec3::ZERO;
            let mut forward = camera_model.forward();
            let mut right = camera_model.right();

            forward.y = 0.0;
            forward = forward.normalize_or_zero();

            right.y = 0.0;
            right = right.normalize_or_zero();

            if input.key(Key::W) {
                del += forward;
            }

            if input.key(Key::S) {
                del -= forward;
            }

            if input.key(Key::D) {
                del += right;
            }

            if input.key(Key::A) {
                del -= right;
            }

            player.ground_timer = player.ground_timer.saturating_sub(tick.0);

            if actor.grounded() && player.jump_timer.is_zero() {
                player.velocity = Vec3::ZERO;
                player.ground_timer = Duration::from_secs_f32(0.1);
            } else {
                player.velocity.y -= 9.82 * dt;
            }

            if input.key_down(Key::Space) && !player.ground_timer.is_zero() {
                player.jump_timer = Duration::from_secs_f32(0.5);
                player.velocity.y = 7.0;
            }

            player.jump_timer = player.jump_timer.saturating_sub(tick.0);

            actor.set_desired_translation(player.velocity + (del.normalize_or_zero() * 6.0));
        }
    }
}

impl PlayerSpawnSystem {
    fn game_start(
        &mut self,
        _: GameStart,
        commands: Commands,
        queries: Queries<(Read<Player>, Read<Model>)>,
        _: Res<()>,
    ) {
        // No need to spawn if we already have a player
        if queries.make::<Read<Player>>().into_iter().len() > 0 {
            return;
        }

        // Find the first player spawn
        if let Some(model) = queries
            .filter()
            .with::<PlayerSpawn>()
            .make::<Read<Model>>()
            .next()
        {
            Player::spawn(
                &commands.entities,
                model.position().into(),
                model.rotation(),
            );
        }
    }
}

impl From<PlayerInputSystem> for System {
    fn from(value: PlayerInputSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(PlayerInputSystem::on_tick)
            .run_before::<Tick, ActorMoveSystem>()
            .build()
    }
}

impl From<PlayerSpawnSystem> for System {
    fn from(value: PlayerSpawnSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(PlayerSpawnSystem::game_start)
            .build()
    }
}
