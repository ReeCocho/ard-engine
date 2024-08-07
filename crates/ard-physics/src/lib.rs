use ard_core::{app::AppBuilder, plugin::Plugin};
use engine::{DynamicsApplySystem, KinematicsApplySystem, PhysicsEngine, PhysicsSystem};

pub use rapier3d::{
    control::KinematicCharacterController,
    prelude::{Isometry, QueryFilter, QueryFilterFlags, SharedShape},
};

pub mod collider;
pub mod engine;
pub mod rigid_body;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(PhysicsEngine::new());
        app.add_system(PhysicsSystem::new());
        app.add_system(DynamicsApplySystem);
        app.add_system(KinematicsApplySystem);
    }
}
