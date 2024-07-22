use ard_engine::{
    ecs::prelude::*,
    physics::{
        collider::{Collider, ColliderHandle},
        engine::PhysicsEngine,
    },
};

use super::{Inspector, InspectorContext};

pub struct ColliderInspector;

impl Inspector for ColliderInspector {
    fn should_inspect(&self, ctx: InspectorContext) -> bool {
        ctx.queries.get::<Read<Collider>>(ctx.entity).is_some()
    }

    fn title(&self) -> &'static str {
        "Collider"
    }

    fn show(&mut self, ctx: InspectorContext) {
        let engine = ctx.res.get_mut::<PhysicsEngine>().unwrap();
        let mut collider = ctx.queries.get::<Write<Collider>>(ctx.entity).unwrap();

        egui::Grid::new("collider_grid")
            .num_columns(2)
            .spacing([30.0, 20.0])
            .striped(true)
            .show(ctx.ui, |ui| {
                ui.label("Mass");
                ui.add(egui::DragValue::new(&mut collider.mass).clamp_range(0.0..=f64::MAX));
                ui.end_row();

                ui.label("Restitution");
                ui.add(egui::DragValue::new(&mut collider.restitution).clamp_range(0.0..=1.0));
                ui.end_row();

                ui.label("Friction");
                ui.add(egui::DragValue::new(&mut collider.friction).clamp_range(0.0..=1.0));
                ui.end_row();
            });

        if let Some(handle) = ctx.queries.get::<Read<ColliderHandle>>(ctx.entity) {
            engine.colliders(|collider_set| {
                let col = match collider_set.get_mut(handle.handle()) {
                    Some(col) => col,
                    None => return,
                };

                col.set_mass(collider.mass);
                col.set_restitution(collider.restitution);
                col.set_friction(collider.friction);
            });
        }
    }
}
