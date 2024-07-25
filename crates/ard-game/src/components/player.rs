use std::time::Duration;

use ard_ecs::prelude::*;
use ard_math::Vec3;

#[derive(Component, Default)]
pub struct Player {
    pub(crate) velocity: Vec3,
    pub(crate) jump_timer: Duration,
    pub(crate) ground_timer: Duration,
}

#[derive(Component)]
pub struct PlayerCamera;
