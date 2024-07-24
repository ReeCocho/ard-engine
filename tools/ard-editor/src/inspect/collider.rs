use ard_engine::{
    ecs::prelude::*,
    math::Vec3,
    physics::{
        collider::{Collider, ColliderHandle, Shape},
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
                ui.add(egui::DragValue::new(&mut collider.mass).clamp_range(0.01..=f64::MAX));
                ui.end_row();

                ui.label("Restitution");
                ui.add(egui::DragValue::new(&mut collider.restitution).clamp_range(0.01..=1.0));
                ui.end_row();

                ui.label("Friction");
                ui.add(egui::DragValue::new(&mut collider.friction).clamp_range(0.0..=1.0));
                ui.end_row();

                ui.label("Offset");
                ui.horizontal(|ui| {
                    ui.label("x");
                    ui.add(egui::DragValue::new(&mut collider.offset.x));
                    ui.label("y");
                    ui.add(egui::DragValue::new(&mut collider.offset.y));
                    ui.label("z");
                    ui.add(egui::DragValue::new(&mut collider.offset.z));
                });
                ui.end_row();

                const DEFAULT_SHAPES: [Shape; 5] = [
                    Shape::default_ball(),
                    Shape::default_box(),
                    Shape::default_capsule(),
                    Shape::default_cone(),
                    Shape::default_cylinder(),
                ];

                ui.label("Shape");
                ui.vertical(|ui| {
                    let mut old_shape = std::mem::discriminant(&collider.shape);
                    egui::ComboBox::new("collider_shape_combo_box", "")
                        .selected_text(collider.shape.name())
                        .show_ui(ui, |ui| {
                            for shape in &DEFAULT_SHAPES {
                                let discrim = std::mem::discriminant(shape);
                                ui.selectable_value(&mut old_shape, discrim, shape.name());
                            }
                        });

                    if old_shape != std::mem::discriminant(&collider.shape) {
                        for shape in &DEFAULT_SHAPES {
                            if std::mem::discriminant(shape) == old_shape {
                                collider.shape = *shape;
                                break;
                            }
                        }
                    }

                    collider_shape_ui(ui, &mut collider.shape);
                });
                ui.end_row();
            });

        if let Some(handle) = ctx.queries.get::<Read<ColliderHandle>>(ctx.entity) {
            engine.colliders(|collider_set| {
                let col = match collider_set.get_mut(handle.handle()) {
                    Some(col) => col,
                    None => return,
                };

                col.set_shape(collider.shape.into());
                col.set_mass(collider.mass);
                col.set_restitution(collider.restitution);
                col.set_friction(collider.friction);
                if col.parent().is_some() {
                    col.set_translation_wrt_parent(collider.offset.into());
                }
            });
        }
    }
}

fn collider_shape_ui(ui: &mut egui::Ui, shape: &mut Shape) {
    match shape {
        Shape::Ball { radius } => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::DragValue::new(radius));
                *radius = radius.max(0.01);
            });
        }
        Shape::Capsule { radius, height } => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::DragValue::new(radius));
                *radius = radius.max(0.01);
            });
            ui.horizontal(|ui| {
                ui.label("Height");
                ui.add(egui::DragValue::new(height));
                *height = height.max(0.01);
            });
        }
        Shape::Box { half_extents } => {
            ui.label("Half Extents");
            ui.horizontal(|ui| {
                ui.label("x");
                ui.add(egui::DragValue::new(&mut half_extents.x));
                ui.label("y");
                ui.add(egui::DragValue::new(&mut half_extents.y));
                ui.label("z");
                ui.add(egui::DragValue::new(&mut half_extents.z));
                *half_extents = half_extents.max(Vec3::ONE * 0.01);
            });
        }
        Shape::Cylinder { height, radius } => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::DragValue::new(radius));
                *radius = radius.max(0.01);
            });
            ui.horizontal(|ui| {
                ui.label("Height");
                ui.add(egui::DragValue::new(height));
                *height = height.max(0.01);
            });
        }
        Shape::Cone { height, radius } => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::DragValue::new(radius));
                *radius = radius.max(0.01);
            });
            ui.horizontal(|ui| {
                ui.label("Height");
                ui.add(egui::DragValue::new(height));
                *height = height.max(0.01);
            });
        }
    }
}
