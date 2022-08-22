use ard_engine::{
    ecs::prelude::*,
    game::{
        components::transform::Transform,
        object::{empty::EmptyObject, static_object::StaticObject},
    },
    math::*,
};

use super::{InspectComponent, Inspectable};

//////////////////
// Game objects //
//////////////////

pub trait InspectGameObject {
    fn inspect(entity: Entity, state: &mut super::InspectState);
}

impl InspectGameObject for EmptyObject {
    fn inspect(entity: Entity, state: &mut super::InspectState) {
        InspectComponent::<Transform>::new("Transform", entity).inspect(state);
    }
}

impl InspectGameObject for StaticObject {
    fn inspect(entity: Entity, state: &mut super::InspectState) {
        InspectComponent::<Transform>::new("Transform", entity).inspect(state);
    }
}

////////////////
// Components //
////////////////

#[derive(Tag, Copy, Clone)]
#[storage(UncommonStorage)]
struct EulerAngleRot(Vec3);

impl Inspectable for Transform {
    fn inspect(&mut self, state: &mut super::InspectState) {
        // Add in the euler angle rotation tag if we don't have it yet
        let mut tag_query = state
            .resources
            .queries
            .get_tag::<(Write<EulerAngleRot>,)>(state.entity);
        let euler_rot = match tag_query.0.as_mut() {
            Some(euler_rot) => euler_rot,
            None => {
                let (x, y, z) = self.rotation.to_euler(EulerRot::XYZ);
                state.resources.ecs_commands.entities.add_tag(
                    state.entity,
                    EulerAngleRot(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())),
                );
                return;
            }
        };

        state.field("Position", &mut self.position);
        state.field("Rotation", &mut euler_rot.0);
        state.field("Scale", &mut self.scale);

        self.rotation = Quat::from_euler(
            EulerRot::XYZ,
            euler_rot.0.x.to_radians(),
            euler_rot.0.y.to_radians(),
            euler_rot.0.z.to_radians(),
        )
        .normalize();
    }
}
