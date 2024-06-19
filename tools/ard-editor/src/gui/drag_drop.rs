use ard_engine::ecs::entity::Entity;

use crate::assets::meta::MetaFile;

pub enum DragDropPayload {
    Asset(MetaFile),
    Entity(Entity),
}
