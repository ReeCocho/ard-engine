use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::components::transform::Transform,
    math::{EulerRot, Quat, Vec3},
};

use super::Inspect;

#[derive(Tag, Copy, Clone)]
#[storage(UncommonStorage)]
struct EulerAngleRot(Vec3);

impl Inspect for Transform {
    fn inspect(
        ui: &imgui::Ui,
        entity: Entity,
        commands: &Commands,
        queries: &Queries<Everything>,
        _: &Assets,
    ) {
        let mut transform = queries.get::<Write<Transform>>(entity).unwrap();

        // Add in the euler angle rotation tag if we don't have it yet
        let mut tag_query = queries.get_tag::<(Write<EulerAngleRot>,)>(entity);
        let mut euler_rot = match tag_query.0.as_mut() {
            Some(euler_rot) => euler_rot,
            None => {
                let (x, y, z) = transform.rotation.to_euler(EulerRot::XYZ);
                commands.entities.add_tag(
                    entity,
                    EulerAngleRot(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())),
                );
                return;
            }
        };

        if ui.collapsing_header("Transform", imgui::TreeNodeFlags::empty()) {
            let mut position: [f32; 3] = transform.position.into();
            let mut rotation: [f32; 3] = euler_rot.0.into();
            let mut scale: [f32; 3] = transform.scale.into();

            ui.input_float3("Position", &mut position).build();
            ui.input_float3("Rotation", &mut rotation).build();
            ui.input_float3("Scale", &mut scale).build();

            euler_rot.0 = rotation.into();
            transform.position = position.into();
            transform.rotation = Quat::from_euler(
                EulerRot::XYZ,
                rotation[0].to_radians(),
                rotation[1].to_radians(),
                rotation[2].to_radians(),
            )
            .normalize();
            transform.scale = scale.into();
        }
    }
}
