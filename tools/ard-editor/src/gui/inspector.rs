use ard_engine::{
    ecs::prelude::*,
    math::{Vec3, Vec4, Vec4Swizzles},
    physics::{
        collider::{CoefficientCombineRule, Collider},
        engine::PhysicsEngine,
        rigid_body::RigidBody,
    },
    render::{shape::Shape, DebugDraw, DebugDrawing, Mesh},
    transform::Model,
};

use crate::{
    inspect::{
        collider::ColliderInspector, rigid_body::RigidBodyInspector, transform::TransformInspector,
        Inspectors,
    },
    selected::Selected,
};

use super::EditorViewContext;

pub struct InspectorView {
    inspectors: Inspectors,
}

impl Default for InspectorView {
    fn default() -> Self {
        let mut inspectors = Inspectors::default();
        inspectors.with(TransformInspector);
        inspectors.with(ColliderInspector);
        inspectors.with(RigidBodyInspector);

        Self { inspectors }
    }
}

impl InspectorView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut selected = ctx.res.get_mut::<Selected>().unwrap();

        let mut phys_engine = ctx.res.get_mut::<PhysicsEngine>().unwrap();
        let mut sim_enabled = phys_engine.simulate();

        ctx.ui.toggle_value(&mut sim_enabled, "Enable Physics");
        phys_engine.set_simulation_enabled(sim_enabled);

        match *selected {
            Selected::None => {}
            Selected::Entity(e) => {
                if ctx.queries.get::<Read<Model>>(e).is_none() {
                    *selected = Selected::None;
                    return egui_tiles::UiResponse::None;
                }
                self.inspect_entity(ctx, &mut selected, e)
            }
        };

        egui_tiles::UiResponse::None
    }

    fn inspect_entity(&mut self, ctx: EditorViewContext, selected: &mut Selected, entity: Entity) {
        if !ctx.queries.is_alive(entity) {
            *selected = Selected::None;
            return;
        }

        if let Some(query) = ctx.queries.get::<(Read<Model>, Read<Mesh>)>(entity) {
            let (model, mesh) = *query;
            let bounds = mesh.bounds();
            ctx.res.get_mut::<DebugDrawing>().unwrap().draw(DebugDraw {
                color: Vec4::new(1.0, 1.0, 0.0, 1.0),
                shape: Shape::Box {
                    min_pt: bounds.min_pt.xyz(),
                    max_pt: bounds.max_pt.xyz(),
                    model: model.0,
                },
            });
        }

        self.inspectors
            .show(ctx.ui, entity, ctx.commands, ctx.queries, ctx.res);

        ctx.ui.menu_button("Add Component", |ui| {
            if ui.button("Collider").clicked() {
                ctx.commands.entities.add_component(
                    entity,
                    Collider {
                        shape: ard_engine::physics::collider::Shape::Box {
                            half_extents: Vec3::new(1.0, 1.0, 1.0),
                        },
                        friction: 0.8,
                        friction_combine_rule: CoefficientCombineRule::Max,
                        restitution: 0.01,
                        restitution_combine_rule: CoefficientCombineRule::Max,
                        mass: 1.0,
                    },
                );
            }

            if ui.button("Rigid Body").clicked() {
                ctx.commands
                    .entities
                    .add_component(entity, RigidBody::default());
            }
        });
    }
}
