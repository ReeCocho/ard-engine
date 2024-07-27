use std::time::Duration;

use ard_ecs::prelude::*;
use ard_math::{Mat4, Quat, Vec3, Vec3A};
use ard_physics::rigid_body::{RigidBody, RigidBodyType};
use ard_render_camera::Camera;
use ard_transform::{Children, Model, Parent, Position, Rotation};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::actor::Actor;

#[derive(Component, Default)]
pub struct Player {
    pub(crate) velocity: Vec3,
    pub(crate) jump_timer: Duration,
    pub(crate) ground_timer: Duration,
}

#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component, Serialize, Deserialize, Clone, Copy)]
pub struct PlayerSpawn;

impl Player {
    pub fn spawn(commands: &EntityCommands, position: Vec3, rotation: Quat) {
        let mut player_ents = [Entity::null(), Entity::null()];
        commands.create_empty(&mut player_ents);

        // Player
        commands.set_components(
            &[player_ents[0]],
            (
                vec![Model(Mat4::from_rotation_translation(rotation, position))],
                vec![Position(position.into())],
                vec![Rotation(rotation)],
                vec![Children(vec![player_ents[1]].into())],
                vec![Actor::default()],
                vec![RigidBody {
                    body_type: RigidBodyType::KinematicPositionBased,
                    ..Default::default()
                }],
                vec![Player::default()],
            ),
        );

        // Camera
        commands.set_components(
            &[player_ents[1]],
            (
                vec![Model(Mat4::IDENTITY)],
                vec![Position(Vec3A::new(0.0, 0.5, 0.0))],
                vec![Rotation(Quat::IDENTITY)],
                vec![Children(SmallVec::default())],
                vec![PlayerCamera],
                vec![Parent(player_ents[0])],
                vec![Camera {
                    order: 1,
                    ..Default::default()
                }],
            ),
        );
    }
}
