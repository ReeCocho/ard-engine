pub mod components;
pub mod game_objects;

use ard_engine::{assets::prelude::*, ecs::prelude::*};

pub trait Inspect {
    fn inspect(
        ui: &imgui::Ui,
        entity: Entity,
        commands: &Commands,
        queries: &Queries<Everything>,
        assets: &Assets,
    );
}
