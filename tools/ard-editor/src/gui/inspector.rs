use ard_engine::{
    ecs::prelude::*,
    math::{Mat4, Vec3, Vec3A, Vec4, Vec4Swizzles},
    physics::{
        collider::{CoefficientCombineRule, Collider},
        engine::PhysicsEngine,
        rigid_body::RigidBody,
    },
    render::{shape::Shape, DebugDraw, DebugDrawing, Mesh},
    transform::Model,
};
use rustc_hash::FxHashMap;

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
    add_component: FxHashMap<String, AddComponentFn>,
}

type AddComponentFn = Box<dyn Fn(Entity, &Commands, &Queries<Everything>, &Res<Everything>)>;

impl Default for InspectorView {
    fn default() -> Self {
        let mut inspectors = Inspectors::default();
        inspectors.with(TransformInspector);
        inspectors.with(ColliderInspector);
        inspectors.with(RigidBodyInspector);

        let mut add_component = FxHashMap::default();
        add_component.insert(
            Collider::NAME.into(),
            add_component_fn(|entity, queries, _| {
                let (shape, offset) = match queries.get::<Read<Mesh>>(entity) {
                    Some(mesh) => {
                        let model = Model(
                            queries
                                .get::<Read<Model>>(entity)
                                .map(|mdl| mdl.0)
                                .unwrap_or(Mat4::IDENTITY),
                        );
                        let scale = Vec3::from(model.scale().abs().max(Vec3A::ONE * 0.05));

                        let bounds = mesh.bounds();
                        let offset = scale * ((bounds.max_pt.xyz() + bounds.min_pt.xyz()) * 0.5);
                        let shape = ard_engine::physics::collider::Shape::Box {
                            half_extents: scale
                                * ((bounds.max_pt.xyz() - bounds.min_pt.xyz()) * 0.5),
                        };

                        (shape, offset)
                    }
                    None => (
                        ard_engine::physics::collider::Shape::Box {
                            half_extents: Vec3::new(2.0, 2.0, 2.0),
                        },
                        Vec3::ZERO,
                    ),
                };

                Collider {
                    shape,
                    offset,
                    friction: 0.8,
                    friction_combine_rule: CoefficientCombineRule::Max,
                    restitution: 0.1,
                    restitution_combine_rule: CoefficientCombineRule::Max,
                    mass: 1.0,
                }
            }),
        );

        add_component.insert(
            RigidBody::NAME.into(),
            add_component_fn(|_, _, _| RigidBody::default()),
        );

        Self {
            inspectors,
            add_component,
        }
    }
}

impl InspectorView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut selected = ctx.res.get_mut::<Selected>().unwrap();

        let mut phys_engine = ctx.res.get_mut::<PhysicsEngine>().unwrap();
        let mut sim_enabled = phys_engine.simulate();

        ctx.ui.toggle_value(&mut sim_enabled, "Enable Physics");
        phys_engine.set_simulation_enabled(sim_enabled);
        std::mem::drop(phys_engine);

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
            for (name, func) in self.add_component.iter() {
                if ui.button(name).clicked() {
                    func(entity, ctx.commands, ctx.queries, ctx.res);
                }
            }
        });
    }
}

fn add_component_fn<C: Component + 'static>(
    func: impl Fn(Entity, &Queries<Everything>, &Res<Everything>) -> C + 'static,
) -> AddComponentFn {
    Box::new(move |entity, commands, queries, res| {
        let component = func(entity, queries, res);
        commands.entities.add_component(entity, component);
    })
}
