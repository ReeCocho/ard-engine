use ard_engine::ecs::entity::Entity;

use crate::assets::EditorAsset;

pub enum DragDropPayload {
    Asset(EditorAsset),
    Entity(Entity),
}
