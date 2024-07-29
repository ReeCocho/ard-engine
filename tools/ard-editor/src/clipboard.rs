use ard_engine::ecs::prelude::*;

use crate::command::entity::TransientEntities;

#[derive(Resource)]
pub enum Clipboard {
    None,
    Entity {
        data: TransientEntities,
        parent: Option<Entity>,
    },
}
