pub use ard_engine::ecs::prelude::*;
use ard_engine::{
    assets::manager::Assets,
    ecs::{component::pack::EmptyComponentPack, tag::pack::EmptyTagPack},
    game::components::destroy::Destroy,
    save_load::{format::Bincode, save_data::SaveData},
};

use crate::scene_graph::SceneGraph;

use super::EditorCommand;

pub struct DestroyEntity {
    entity: Entity,
    entity_and_children: Vec<Entity>,
    saved: SaveData,
    external_entities: Vec<Entity>,
}

impl DestroyEntity {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            entity_and_children: Vec::default(),
            saved: SaveData::default(),
            external_entities: Vec::default(),
        }
    }
}

impl EditorCommand for DestroyEntity {
    fn apply(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        if self.entity_and_children.is_empty() {
            self.entity_and_children = SceneGraph::collect_children(queries, vec![self.entity]);
        }

        let assets = res.get::<Assets>().unwrap().clone();
        let (saved, entity_map) =
            crate::ser::saver::<Bincode>().save(assets, queries, &self.entity_and_children);
        let external_entities: Vec<_> = entity_map.mapped()[self.entity_and_children.len()..]
            .iter()
            .map(|e| *e)
            .collect();

        self.saved = saved;
        self.external_entities = external_entities;

        commands.entities.set_components(
            &self.entity_and_children,
            EmptyComponentPack {
                count: self.entity_and_children.len(),
            },
        );
        commands.entities.set_tags(
            &self.entity_and_children,
            EmptyTagPack {
                count: self.entity_and_children.len(),
            },
        )
    }

    fn undo(&mut self, commands: &Commands, _queries: &Queries<Everything>, res: &Res<Everything>) {
        let data = std::mem::take(&mut self.saved);
        let assets = res.get::<Assets>().unwrap().clone();
        crate::ser::loader::<Bincode>().load_with_external(
            data,
            assets,
            &commands.entities,
            Some(&self.entity_and_children),
            &self.external_entities,
        );
    }

    fn clear(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
    }
}
