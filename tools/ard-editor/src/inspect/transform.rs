use std::ops::DerefMut;

use ard_engine::{
    core::stat::Static,
    ecs::prelude::*,
    math::{EulerRot, Quat, Vec3A},
    transform::{Position, Rotation, Scale},
};

use super::{Inspector, InspectorContext};

pub struct TransformInspector;

#[derive(Tag)]
#[storage(UncommonStorage)]
pub struct EulerRotation(pub Vec3A);

impl Inspector for TransformInspector {
    fn title(&self) -> &'static str {
        "Transform"
    }

    fn should_inspect(&self, _ctx: InspectorContext) -> bool {
        true
    }

    fn show(&mut self, ctx: InspectorContext) {
        let position = ctx.queries.get::<Write<Position>>(ctx.entity);
        let rotation = ctx
            .queries
            .get::<(Entity, Write<Rotation>, Write<EulerRotation>)>(ctx.entity);
        let scale = ctx.queries.get::<Write<Scale>>(ctx.entity);
        let enabled = ctx.queries.get::<Read<Static>>(ctx.entity).is_none();

        ctx.ui.add_enabled_ui(enabled, |ui| {
            egui::Grid::new("transform_grid")
                .num_columns(2)
                .spacing([30.0, 20.0])
                .striped(true)
                .show(ui, |ui| {
                    if let Some(mut position) = position {
                        ui.label("Position");
                        ui.horizontal(|ui| {
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut position.0.x));
                            ui.label("y");
                            ui.add(egui::DragValue::new(&mut position.0.y));
                            ui.label("z");
                            ui.add(egui::DragValue::new(&mut position.0.z));
                        });

                        ui.end_row();
                    }

                    if let Some(mut query) = rotation {
                        let (rotation, euler_rot) = query.deref_mut();

                        let euler_rot = match euler_rot {
                            Some(euer_rot) => euer_rot,
                            None => {
                                let (y, x, z) = rotation.0.to_euler(EulerRot::YXZ);
                                let new_euler_rot = EulerRotation(Vec3A::new(
                                    x.to_degrees(),
                                    y.to_degrees(),
                                    z.to_degrees(),
                                ));
                                ctx.commands.entities.add_tag(ctx.entity, new_euler_rot);
                                return;
                            }
                        };

                        let orig = euler_rot.0;

                        ui.label("Rotation");
                        ui.horizontal(|ui| {
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut euler_rot.0.x));
                            ui.label("y");
                            ui.add(egui::DragValue::new(&mut euler_rot.0.y));
                            ui.label("z");
                            ui.add(egui::DragValue::new(&mut euler_rot.0.z));
                        });

                        if orig != euler_rot.0 {
                            rotation.0 = Quat::from_euler(
                                EulerRot::YXZ,
                                euler_rot.0.y.to_radians(),
                                euler_rot.0.x.to_radians(),
                                euler_rot.0.z.to_radians(),
                            );
                        }

                        ui.end_row();
                    }

                    if let Some(mut scale) = scale {
                        ui.label("Scale");
                        ui.horizontal(|ui| {
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut scale.0.x));
                            ui.label("y");
                            ui.add(egui::DragValue::new(&mut scale.0.y));
                            ui.label("z");
                            ui.add(egui::DragValue::new(&mut scale.0.z));
                        });

                        ui.end_row();
                    }
                });
        });
    }
}
