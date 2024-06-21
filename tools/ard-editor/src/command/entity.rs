pub use ard_engine::ecs::prelude::*;
use ard_engine::{
    assets::manager::Assets,
    core::{
        core::Name,
        stat::{DirtyStatic, Static, StaticGroup},
    },
    ecs::{component::pack::EmptyComponentPack, tag::pack::EmptyTagPack},
    game::components::{
        destroy::Destroy,
        transform::{Children, Parent, Position, Rotation, Scale, SetParent},
    },
    math::Vec3,
    render::Model,
    save_load::{format::Bincode, save_data::SaveData},
};
use rustc_hash::FxHashSet;

use crate::{camera::SceneViewCamera, scene_graph::SceneGraph, selected::Selected};

use super::EditorCommand;

pub const INSTANTIATE_DISTANCE: f32 = 8.0;

pub struct DestroyEntity {
    entity: Entity,
    transient: TransientEntities,
}

#[derive(Default)]
pub struct CreateEmptyEntity {
    position: Option<Vec3>,
    entity: Entity,
}

pub struct SetParentCommand {
    entity: Entity,
    new_parent: Option<Entity>,
    old_parent: Option<Entity>,
    new_index: usize,
    old_index: usize,
}

#[derive(Default)]
pub struct TransientEntities {
    external_entities: Vec<Entity>,
    internal_entities: Vec<Entity>,
    static_groups: FxHashSet<StaticGroup>,
    saved: SaveData,
}

impl DestroyEntity {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            transient: TransientEntities::default(),
        }
    }
}

impl EditorCommand for DestroyEntity {
    fn apply(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        let entities = SceneGraph::collect_children(queries, vec![self.entity]);
        self.transient = TransientEntities::new(&entities, queries, res);
        self.transient.store(commands, res);
    }

    fn undo(&mut self, commands: &Commands, _queries: &Queries<Everything>, res: &Res<Everything>) {
        std::mem::take(&mut self.transient).reload(commands, res);
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

impl EditorCommand for SetParentCommand {
    fn apply(
        &mut self,
        commands: &Commands,
        queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        if self.will_create_loop(queries) {
            return;
        }

        commands.entities.add_component(
            self.entity,
            SetParent {
                new_parent: self.new_parent,
                index: self.new_index,
            },
        );

        std::mem::swap(&mut self.new_parent, &mut self.old_parent);
        std::mem::swap(&mut self.new_index, &mut self.old_index);
    }

    fn undo(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        self.apply(commands, queries, res);
    }
}

impl SetParentCommand {
    pub fn new(
        entity: Entity,
        new_parent: Option<Entity>,
        index: usize,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Self {
        let (old_parent, old_index) = queries
            .get::<Read<Parent>>(entity)
            .map(|old_parent| {
                let old_index = queries
                    .get::<Read<Children>>(old_parent.0)
                    .and_then(|parents_children| parents_children.index_of(entity))
                    .unwrap_or_default();
                (Some(old_parent.0), old_index)
            })
            .unwrap_or_else(|| {
                let old_index = res
                    .get::<SceneGraph>()
                    .unwrap()
                    .find_in_roots(entity)
                    .unwrap_or_default();
                (None, old_index)
            });

        Self {
            entity,
            new_parent,
            new_index: index,
            old_parent,
            old_index,
        }
    }

    fn will_create_loop(&self, queries: &Queries<Everything>) -> bool {
        let mut new_parent = self.new_parent;
        while let Some(parent) = new_parent {
            if parent == self.entity {
                return true;
            }
            new_parent = queries.get::<Read<Parent>>(parent).map(|p| p.0);
        }
        false
    }
}

impl TransientEntities {
    pub fn new(entities: &[Entity], queries: &Queries<Everything>, res: &Res<Everything>) -> Self {
        let internal_entities = Vec::from_iter(entities.iter().cloned());

        let assets = res.get::<Assets>().unwrap().clone();
        let (saved, entity_map) = crate::ser::saver::<Bincode>()
            .save(assets, queries, &internal_entities)
            .unwrap();

        let external_entities: Vec<_> = entity_map.mapped()[internal_entities.len()..]
            .iter()
            .cloned()
            .collect();

        let static_groups: FxHashSet<_> = internal_entities
            .iter()
            .flat_map(|entity| queries.get::<Read<Static>>(*entity).map(|s| s.0))
            .collect();

        Self {
            internal_entities,
            external_entities,
            saved,
            static_groups,
        }
    }

    #[inline(always)]
    pub fn internal_entities(&self) -> &[Entity] {
        &self.internal_entities
    }

    pub fn store(&mut self, commands: &Commands, res: &Res<Everything>) {
        let dirty_static = res.get::<DirtyStatic>().unwrap();
        self.static_groups.iter().for_each(|group| {
            dirty_static.signal(*group);
        });

        commands.entities.set_components(
            &self.internal_entities,
            EmptyComponentPack {
                count: self.internal_entities.len(),
            },
        );
        commands.entities.set_tags(
            &self.internal_entities,
            EmptyTagPack {
                count: self.internal_entities.len(),
            },
        );
    }

    pub fn reload(self, commands: &Commands, res: &Res<Everything>) {
        let dirty_static = res.get::<DirtyStatic>().unwrap();
        self.static_groups.iter().for_each(|group| {
            dirty_static.signal(*group);
        });

        let assets = res.get::<Assets>().unwrap().clone();

        crate::ser::loader::<Bincode>()
            .load_with_external(
                self.saved,
                assets,
                &commands.entities,
                Some(&self.internal_entities),
                &self.external_entities,
            )
            .unwrap();
    }
}
