use ard_engine::{ecs::prelude::*, game::components::player::PlayerSpawn};

use super::{Inspector, InspectorContext};

pub struct PlayerSpawnInspector;

impl Inspector for PlayerSpawnInspector {
    fn should_inspect(&self, ctx: InspectorContext) -> bool {
        ctx.queries.get::<Read<PlayerSpawn>>(ctx.entity).is_some()
    }

    fn title(&self) -> &'static str {
        "Player Spawn"
    }

    fn show(&mut self, _ctx: InspectorContext) {}

    fn remove(&mut self, ctx: InspectorContext) {
        ctx.commands
            .entities
            .remove_component::<PlayerSpawn>(ctx.entity);
    }
}
