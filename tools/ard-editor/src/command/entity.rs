pub use ard_engine::ecs::prelude::*;
use ard_engine::{
    assets::manager::Assets,
    core::core::Name,
    ecs::{component::pack::EmptyComponentPack, tag::pack::EmptyTagPack},
    game::components::{
        destroy::Destroy,
        transform::{Children, Position, Rotation, Scale},
    },
    math::Vec3,
    render::Model,
    save_load::{format::Bincode, save_data::SaveData},
};

use crate::{camera::SceneViewCamera, scene_graph::SceneGraph, selected::Selected};

use super::EditorCommand;

pub const INSTANTIATE_DISTANCE: f32 = 8.0;

pub struct DestroyEntity {
    entity: Entity,
    entity_and_children: Vec<Entity>,
    saved: SaveData,
    external_entities: Vec<Entity>,
}

#[derive(Default)]
pub struct CreateEmptyEntity {
    position: Option<Vec3>,
    entity: Entity,
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
        let (saved, entity_map) = crate::ser::saver::<Bincode>()
            .save(assets, queries, &self.entity_and_children)
            .unwrap();
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
        crate::ser::loader::<Bincode>()
            .load_with_external(
                data,
                assets,
                &commands.entities,
                Some(&self.entity_and_children),
                &self.external_entities,
            )
            .unwrap();
    }
}

impl EditorCommand for CreateEmptyEntity {
    fn apply(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        match self.position {
            None => {
                let camera_model = queries
                    .get::<Read<Model>>(res.get::<SceneViewCamera>().unwrap().camera())
                    .unwrap();
                let position = Vec3::from(camera_model.position())
                    + (camera_model.forward() * INSTANTIATE_DISTANCE);

                commands.entities.create(
                    (
                        vec![Model::default()],
                        vec![Position(position.into())],
                        vec![Rotation::default()],
                        vec![Scale::default()],
                        vec![Children::default()],
                        vec![Name("New Entity".into())],
                    ),
                    std::slice::from_mut(&mut self.entity),
                );

                *res.get_mut::<Selected>().unwrap() = Selected::Entity(self.entity);

                self.position = Some(position);
            }
            Some(position) => {
                commands.entities.set_components(
                    &[self.entity],
                    (
                        vec![Model::default()],
                        vec![Position(position.into())],
                        vec![Rotation::default()],
                        vec![Scale::default()],
                        vec![Children::default()],
                        vec![Name("New Entity".into())],
                    ),
                );
            }
        }
    }

    fn undo(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        commands
            .entities
            .set_components(&[self.entity], EmptyComponentPack { count: 1 })
    }

    fn clear(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        commands.entities.add_component(self.entity, Destroy);
    }
}
