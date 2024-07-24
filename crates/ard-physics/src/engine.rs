use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ard_core::{
    core::Tick,
    destroy::{Destroy, Destroyer},
};
use ard_ecs::{prelude::*, system::data::SystemData};
use ard_math::{Mat4, Quat, Vec3A};
use ard_transform::{system::ModelUpdateSystem, Model, Parent, Position, Rotation};
use rapier3d::{
    dynamics::{
        CCDSolver, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet,
        RigidBodyBuilder, RigidBodySet,
    },
    geometry::{ColliderBuilder, ColliderSet, DefaultBroadPhase, NarrowPhase},
    math::Isometry,
    pipeline::{PhysicsPipeline, QueryPipeline},
};

use crate::{
    collider::{Collider, ColliderHandle},
    rigid_body::{RigidBody, RigidBodyHandle},
};

pub const SIMULATION_RATE: f32 = 1.0 / 30.0;

#[derive(Event, Clone, Copy)]
pub struct PhysicsStep(pub Duration);

#[derive(Clone, Resource)]
pub struct PhysicsEngine(pub(crate) Arc<Mutex<PhysicsEngineInner>>);

pub(crate) struct PhysicsEngineInner {
    pub simulate: bool,
    pub interpolation_rate: f32,
    pub physics_pipeline: PhysicsPipeline,
    pub query_pipeline: QueryPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub ccd_solver: CCDSolver,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub rigid_bodies: RigidBodySet,
}

#[derive(SystemState)]
pub struct PhysicsSystem {
    elapsed: Duration,
}

#[derive(SystemState)]
pub struct DynamicsApplySystem;

#[derive(SystemState)]
pub struct KinematicsApplySystem;

impl PhysicsEngine {
    pub fn new() -> Self {
        let physics_pipeline = PhysicsPipeline::new();
        let island_manager = IslandManager::new();
        let broad_phase = DefaultBroadPhase::new();
        let narrow_phase = NarrowPhase::new();
        let ccd_solver = CCDSolver::new();
        let query_pipeline = QueryPipeline::new();

        Self(Arc::new(Mutex::new(PhysicsEngineInner {
            simulate: false,
            interpolation_rate: 20.0,
            physics_pipeline,
            query_pipeline,
            island_manager,
            broad_phase,
            narrow_phase,
            ccd_solver,
            rigid_bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
        })))
    }

    #[inline(always)]
    pub fn simulate(&self) -> bool {
        self.0.lock().unwrap().simulate
    }

    #[inline(always)]
    pub fn colliders<R>(&self, func: impl FnOnce(&mut ColliderSet) -> R) -> R {
        let mut inner = self.0.lock().unwrap();
        func(&mut inner.colliders)
    }

    #[inline(always)]
    pub fn rigid_bodies<R>(&self, func: impl FnOnce(&mut RigidBodySet) -> R) -> R {
        let mut inner = self.0.lock().unwrap();
        func(&mut inner.rigid_bodies)
    }

    #[inline(always)]
    pub fn set_simulation_enabled(&mut self, enabled: bool) {
        self.0.lock().unwrap().simulate = enabled;
    }
}

impl PhysicsSystem {
    pub fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
        }
    }

    pub fn tick(
        &mut self,
        event: Tick,
        commands: Commands,
        queries: Queries<(
            Read<Model>,
            Read<RigidBody>,
            Read<RigidBodyHandle>,
            Read<Collider>,
            Read<ColliderHandle>,
        )>,
        res: Res<(Write<PhysicsEngine>,)>,
    ) {
        let engine = res.get_mut::<PhysicsEngine>().unwrap();
        let engine_outer = engine.clone();
        let mut engine = engine.0.lock().unwrap();
        let engine = &mut *engine;

        if !engine.simulate {
            return;
        }

        self.check_for_destroyed_entities(
            &queries,
            &mut engine.island_manager,
            &mut engine.colliders,
            &mut engine.rigid_bodies,
        );

        self.check_for_new_entities(
            &queries,
            &commands.entities,
            engine_outer,
            &mut engine.colliders,
            &mut engine.rigid_bodies,
        );

        // Check for a physics step
        self.elapsed += event.0;
        let elapsed_steps = (self.elapsed.as_secs_f32() / SIMULATION_RATE)
            .floor()
            .max(0.0) as u32;

        if elapsed_steps > 0 {
            let del = Duration::from_secs_f32(elapsed_steps as f32 * SIMULATION_RATE);
            commands.events.submit(PhysicsStep(del));
            self.elapsed -= del;
        }
    }

    pub fn phys_step(
        &mut self,
        event: PhysicsStep,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<PhysicsEngine>,)>,
    ) {
        let engine = res.get_mut::<PhysicsEngine>().unwrap();
        let mut engine = engine.0.lock().unwrap();
        let engine = &mut *engine;

        let integration_parameters = IntegrationParameters {
            dt: event.0.as_secs_f32(),
            ..Default::default()
        };

        let gravity = nalgebra::vector![0.0, -9.81, 0.0];
        let physics_hooks = ();
        let event_handler = ();

        engine.physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut engine.island_manager,
            &mut engine.broad_phase,
            &mut engine.narrow_phase,
            &mut engine.rigid_bodies,
            &mut engine.colliders,
            &mut engine.impulse_joints,
            &mut engine.multibody_joints,
            &mut engine.ccd_solver,
            Some(&mut engine.query_pipeline),
            &physics_hooks,
            &event_handler,
        );
    }

    fn check_for_destroyed_entities(
        &self,
        queries: &Queries<impl SystemData>,
        island_manager: &mut IslandManager,
        colliders: &mut ColliderSet,
        rigid_bodies: &mut RigidBodySet,
    ) {
        let mut impulse_joint_set = ImpulseJointSet::new();
        let mut multibody_joint_set = MultibodyJointSet::new();

        for handle in queries
            .filter()
            .without::<Destroy>()
            .without::<RigidBody>()
            .make::<Read<RigidBodyHandle>>()
        {
            rigid_bodies.remove(
                handle.handle(),
                island_manager,
                colliders,
                &mut impulse_joint_set,
                &mut multibody_joint_set,
                false,
            );
        }

        for handle in queries
            .filter()
            .without::<Destroy>()
            .without::<Collider>()
            .make::<Read<ColliderHandle>>()
        {
            colliders.remove(handle.handle(), island_manager, rigid_bodies, false);
        }

        for handle in queries
            .filter()
            .with::<Destroy>()
            .make::<Read<RigidBodyHandle>>()
        {
            rigid_bodies.remove(
                handle.handle(),
                island_manager,
                colliders,
                &mut impulse_joint_set,
                &mut multibody_joint_set,
                false,
            );
        }

        for handle in queries
            .filter()
            .with::<Destroy>()
            .make::<Read<ColliderHandle>>()
        {
            colliders.remove(handle.handle(), island_manager, rigid_bodies, false);
        }
    }

    fn check_for_new_entities(
        &self,
        queries: &Queries<impl SystemData>,
        commands: &EntityCommands,
        engine: PhysicsEngine,
        colliders: &mut ColliderSet,
        rigid_bodies: &mut RigidBodySet,
    ) {
        for (entity, (collider, rigid_body, model)) in queries
            .filter()
            .without::<Destroy>()
            .without::<RigidBodyHandle>()
            .without::<ColliderHandle>()
            .make::<(
                Entity,
                (Read<Collider>, Option<Read<RigidBody>>, Option<Read<Model>>),
            )>()
        {
            let model = model.cloned().unwrap_or(Model(Mat4::IDENTITY));

            let rigid_body_handle = rigid_body.map(|rb| {
                let rb = RigidBodyBuilder::new(rb.body_type)
                    .position(Isometry::from_parts(
                        model.position().into(),
                        model.rotation().into(),
                    ))
                    .gravity_scale(rb.gravity_scale)
                    .linear_damping(rb.linear_damping)
                    .angular_damping(rb.angular_damping)
                    .can_sleep(rb.can_sleep)
                    .ccd_enabled(rb.ccd_enabled)
                    .soft_ccd_prediction(rb.soft_ccd_prediction)
                    .build();
                rigid_bodies.insert(rb)
            });

            let (col_pos, col_rot) = if rigid_body_handle.is_some() {
                (collider.offset, Quat::IDENTITY)
            } else {
                let col_model = Model(model.0 * Mat4::from_translation(collider.offset));
                (col_model.position().into(), col_model.rotation())
            };

            let col = ColliderBuilder::new(collider.shape.into())
                .position(Isometry::from_parts(col_pos.into(), col_rot.into()))
                .friction(collider.friction)
                .friction_combine_rule(collider.friction_combine_rule)
                .restitution(collider.restitution)
                .restitution_combine_rule(collider.restitution_combine_rule)
                .mass(collider.mass)
                .build();

            let collider_handle = match rigid_body_handle {
                Some(rb_handle) => {
                    commands.add_component(entity, RigidBodyHandle::new(rb_handle, engine.clone()));
                    colliders.insert_with_parent(col, rb_handle, rigid_bodies)
                }
                None => colliders.insert(col),
            };

            commands.add_component(entity, ColliderHandle::new(collider_handle, engine.clone()));
        }
    }
}

impl DynamicsApplySystem {
    fn on_tick(
        &mut self,
        tick: Tick,
        _: Commands,
        queries: Queries<(
            Read<RigidBodyHandle>,
            Write<Model>,
            Write<Position>,
            Write<Rotation>,
            Read<Parent>,
        )>,
        res: Res<(Read<PhysicsEngine>,)>,
    ) {
        let engine = res.get::<PhysicsEngine>().unwrap();
        let engine = engine.0.lock().unwrap();

        if !engine.simulate {
            return;
        }

        let lerp = (tick.0.as_secs_f32() * engine.interpolation_rate).min(1.0);

        // First, construct the new model matrices of every object with a rigid body
        for (model, rb_handle) in queries.make::<(Write<Model>, Read<RigidBodyHandle>)>() {
            let rb = match engine.rigid_bodies.get(rb_handle.handle()) {
                Some(rb) => rb,
                None => continue,
            };

            if rb.is_kinematic() {
                continue;
            }

            let global_scale: Vec3A = model.scale();
            let global_pos: Vec3A = model.position().lerp(rb.translation().xyz().into(), lerp);
            let global_rot: Quat = model.rotation().slerp((*rb.rotation()).into(), lerp);

            model.0 = Mat4::from_scale_rotation_translation(
                global_scale.into(),
                global_rot.into(),
                global_pos.into(),
            );
        }

        // Then, compute what the local position and rotations should be
        for (parent, model, position, rotation) in
            queries.filter().with::<RigidBodyHandle>().make::<(
                Option<Read<Parent>>,
                Read<Model>,
                Option<Write<Position>>,
                Option<Write<Rotation>>,
            )>()
        {
            let parent_model_inv = parent
                .map(|parent| {
                    queries
                        .get::<Read<Model>>(parent.0)
                        .map(|mdl| mdl.0.inverse())
                        .unwrap_or(Mat4::IDENTITY)
                })
                .unwrap_or(Mat4::IDENTITY);

            let local_model = Model(parent_model_inv * model.0);

            if let Some(pos) = position {
                pos.0 = local_model.position();
            }

            if let Some(rot) = rotation {
                rot.0 = local_model.rotation();
            }
        }
    }
}

impl KinematicsApplySystem {
    fn on_tick(
        &mut self,
        _: Tick,
        _: Commands,
        queries: Queries<(Read<RigidBodyHandle>, Read<Model>)>,
        res: Res<(Read<PhysicsEngine>,)>,
    ) {
        let engine = res.get::<PhysicsEngine>().unwrap();
        let mut engine = engine.0.lock().unwrap();
        if engine.simulate {
            for (model, rb_handle) in queries.make::<(Read<Model>, Read<RigidBodyHandle>)>() {
                let rb = match engine.rigid_bodies.get_mut(rb_handle.handle()) {
                    Some(rb) => rb,
                    None => continue,
                };

                if !rb.is_kinematic() {
                    continue;
                }

                rb.set_next_kinematic_rotation(model.rotation().into());
                rb.set_next_kinematic_translation(model.position().into());
            }
        } else {
            for (model, rb_handle) in queries.make::<(Read<Model>, Read<RigidBodyHandle>)>() {
                let rb = match engine.rigid_bodies.get_mut(rb_handle.handle()) {
                    Some(rb) => rb,
                    None => continue,
                };

                rb.set_rotation(model.rotation().into(), true);
                rb.set_translation(model.position().into(), true);
            }
        }
    }
}

impl From<PhysicsSystem> for System {
    fn from(value: PhysicsSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(PhysicsSystem::tick)
            .with_handler(PhysicsSystem::phys_step)
            .run_before::<Tick, Destroyer>()
            .run_after::<Tick, ModelUpdateSystem>()
            .build()
    }
}

impl From<DynamicsApplySystem> for System {
    fn from(value: DynamicsApplySystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(DynamicsApplySystem::on_tick)
            .run_before::<Tick, ModelUpdateSystem>()
            .build()
    }
}

impl From<KinematicsApplySystem> for System {
    fn from(value: KinematicsApplySystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(KinematicsApplySystem::on_tick)
            .run_after::<Tick, ModelUpdateSystem>()
            .build()
    }
}
