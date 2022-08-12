use ard_assets::prelude::Handle;
use ard_ecs::prelude::*;
use ard_game_derive::SaveLoad;
use ard_graphics_assets::prelude::ModelAsset;
use serde::{Deserialize, Serialize};

use crate::serialization::SaveLoad;

#[derive(Default, SaveLoad, Component)]
pub struct RenderableData {
    pub source: Option<RenderableSource>,
}

#[derive(Default, SaveLoad)]
pub enum RenderableSource {
    #[default]
    None,
    Model {
        model: Handle<ModelAsset>,
        mesh_group_idx: usize,
        mesh_idx: usize,
    },
}
