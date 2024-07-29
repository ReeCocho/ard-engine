use ard_engine::{
    ecs::prelude::*,
    physics::{
        engine::PhysicsEngine,
        rigid_body::{RigidBody, RigidBodyHandle},
    },
};

use super::{Inspector, InspectorContext};

pub struct RigidBodyInspector;

impl Inspector for RigidBodyInspector {
    fn should_inspect(&self, ctx: InspectorContext) -> bool {
        ctx.queries.get::<Read<RigidBody>>(ctx.entity).is_some()
    }

    fn title(&self) -> &'static str {
        "Rigid Body"
    }

    fn show(&mut self, ctx: InspectorContext) {
        let phys_engine = ctx.res.get_mut::<PhysicsEngine>().unwrap();
        let _rigid_body = ctx.queries.get::<Write<RigidBody>>(ctx.entity).unwrap();

        egui::Grid::new("rigid_body_grid")
            .num_columns(2)
            .spacing([30.0, 20.0])
            .striped(true)
            .show(ctx.ui, |_ui| {});

        if let Some(handle) = ctx.queries.get::<Read<RigidBodyHandle>>(ctx.entity) {
            phys_engine.rigid_bodies(|rigid_bodies| {
                let _rb = match rigid_bodies.get_mut(handle.handle()) {
                    Some(rb) => rb,
                    None => return,
                };
            });
        }
    }

    fn remove(&mut self, ctx: InspectorContext) {
        ctx.commands
            .entities
            .remove_component::<RigidBody>(ctx.entity);
        ctx.commands
            .entities
            .remove_component::<RigidBodyHandle>(ctx.entity);
    }
}
