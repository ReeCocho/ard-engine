use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::{
        components::transform::Transform,
        object::{empty::EmptyObject, static_object::StaticObject},
    },
};

use super::Inspect;

impl Inspect for EmptyObject {
    fn inspect(
        ui: &imgui::Ui,
        entity: Entity,
        commands: &Commands,
        queries: &Queries<Everything>,
        assets: &Assets,
    ) {
        Transform::inspect(ui, entity, commands, queries, assets);
    }
}

impl Inspect for StaticObject {
    fn inspect(
        ui: &imgui::Ui,
        entity: Entity,
        commands: &Commands,
        queries: &Queries<Everything>,
        assets: &Assets,
    ) {
        Transform::inspect(ui, entity, commands, queries, assets);
    }
}
