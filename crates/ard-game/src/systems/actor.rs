use ard_core::{
    core::{Disabled, Tick},
    prelude::Destroy,
};
use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec3};
use ard_physics::{
    engine::{KinematicsApplySystem, PhysicsEngine},
    rigid_body::RigidBodyHandle,
    Isometry,
};
use ard_transform::{system::ModelUpdateSystem, Model};

use crate::components::actor::Actor;

#[derive(SystemState)]
pub struct ActorMoveSystem;

impl ActorMoveSystem {
    fn tick(
        &mut self,
        tick: Tick,
        _: Commands,
        queries: Queries<(
            Entity,
            (Write<Model>, Read<RigidBodyHandle>, Write<Actor>),
            Read<Disabled>,
        )>,
        res: Res<(Write<PhysicsEngine>,)>,
    ) {
        let dt = tick.0.as_secs_f32();
        let phys_engine = res.get::<PhysicsEngine>().unwrap();
        let phys_engine = phys_engine.inner();

        for (_, (model, rb_handle, actor), disabled) in
            queries.filter().without::<Destroy>().make::<(
                Entity,
                (Write<Model>, Read<RigidBodyHandle>, Write<Actor>),
                Read<Disabled>,
            )>()
        {
            if disabled.is_some() {
                continue;
            }

            let shape: ard_physics::SharedShape = actor.shape().clone().into();
            let movement = actor.controller().move_shape(
                dt,
                &phys_engine.rigid_bodies,
                &phys_engine.colliders,
                &phys_engine.query_pipeline,
                shape.0.as_ref(),
                &Isometry::new(model.position().into(), Vec3::Y.into()),
                (actor.desired_translation() * dt).into(),
                ard_physics::QueryFilter::default().exclude_rigid_body(rb_handle.handle()),
                |_| {},
            );

            actor.grounded = movement.grounded;
            model.0 = Mat4::from_scale_rotation_translation(
                model.scale().into(),
                model.rotation(),
                Vec3::from(model.position()) + Vec3::from(movement.translation),
            );
        }
    }
}

impl From<ActorMoveSystem> for System {
    fn from(value: ActorMoveSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(ActorMoveSystem::tick)
            .run_before::<Tick, KinematicsApplySystem>()
            .run_after::<Tick, ModelUpdateSystem>()
            .build()
    }
}
