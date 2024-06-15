use std::ops::DerefMut;

use ard_engine::{
    ecs::prelude::*,
    game::components::transform::{Position, Rotation, Scale},
    math::{EulerRot, Quat, Vec3A},
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

        if let Some(mut position) = position {
            ctx.ui.label("Position");
            ctx.ui.horizontal(|ui| {
                ui.label("x");
                ui.add(egui::DragValue::new(&mut position.0.x));
                ui.label("y");
                ui.add(egui::DragValue::new(&mut position.0.y));
                ui.label("z");
                ui.add(egui::DragValue::new(&mut position.0.z));
            });
        }

        if let Some(mut query) = rotation {
            let (rotation, euler_rot) = query.deref_mut();

            let euler_rot = match euler_rot {
                Some(euer_rot) => euer_rot,
                None => {
                    let (y, x, z) = rotation.0.to_euler(EulerRot::YXZ);
                    let new_euler_rot =
                        EulerRotation(Vec3A::new(x.to_degrees(), y.to_degrees(), z.to_degrees()));
                    ctx.commands.entities.add_tag(ctx.entity, new_euler_rot);
                    return;
                }
            };

            let orig = euler_rot.0;

            ctx.ui.label("Rotation");
            ctx.ui.horizontal(|ui| {
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
        }

        if let Some(mut scale) = scale {
            ctx.ui.label("Scale");
            ctx.ui.horizontal(|ui| {
                ui.label("x");
                ui.add(egui::DragValue::new(&mut scale.0.x));
                ui.label("y");
                ui.add(egui::DragValue::new(&mut scale.0.y));
                ui.label("z");
                ui.add(egui::DragValue::new(&mut scale.0.z));
            });
        }
    }
}
