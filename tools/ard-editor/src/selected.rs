use ard_engine::ecs::prelude::*;

#[derive(Resource, Default)]
pub enum Selected {
    #[default]
    None,
    Entity(Entity),
}
