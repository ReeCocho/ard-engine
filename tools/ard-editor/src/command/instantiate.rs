use crate::{scene_graph::SceneGraph, tasks::instantiate::InstantiateAssetHandle};
use ard_engine::{
    assets::manager::Assets,
    core::core::Name,
    ecs::prelude::*,
    game::components::transform::{Children, Parent, Position, Rotation, Scale},
    math::Mat4,
    render::{
        loader::{MaterialHandle, MeshHandle},
        model::{ModelAsset, Node, NodeData},
        Model, RenderFlags,
    },
};

use super::{entity::TransientEntities, EditorCommand};

pub struct InstantiateCommand {
    handle: InstantiateAssetHandle,
    roots: Vec<Entity>,
    transient: TransientEntities,
}

impl InstantiateCommand {
    pub fn new(handle: InstantiateAssetHandle) -> Self {
        Self {
            handle,
            roots: Vec::default(),
            transient: TransientEntities::default(),
        }
    }
}

impl EditorCommand for InstantiateCommand {
    fn apply(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        match &self.handle {
            InstantiateAssetHandle::Model(handle) => {
                let assets = res.get::<Assets>().unwrap();
                let model = assets.get(&handle).unwrap();
                self.roots = instantiate_model(&model, commands, &assets);
            }
        }
    }

    fn undo(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        let entities = SceneGraph::collect_children(queries, self.roots.clone());
        self.transient = TransientEntities::new(&entities, queries, res);
        self.transient.store(commands, res);
    }

    fn redo(&mut self, commands: &Commands, _queries: &Queries<Everything>, res: &Res<Everything>) {
        std::mem::take(&mut self.transient).reload(commands, res);
    }

    fn clear(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        commands
            .entities
            .destroy(self.transient.internal_entities());
    }
}

fn instantiate_model(model: &ModelAsset, commands: &Commands, assets: &Assets) -> Vec<Entity> {
    struct EmptyInstance {
        model: Model,
        position: Position,
        rotation: Rotation,
        scale: Scale,
        children: Children,
        parent: Option<Parent>,
        name: Option<Name>,
    }

    struct MeshInstance {
        mesh: MeshHandle,
        material: MaterialHandle,
        flags: RenderFlags,
        model: Model,
        position: Position,
        rotation: Rotation,
        scale: Scale,
        children: Children,
        parent: Option<Parent>,
        name: Option<Name>,
    }

    enum ObjectInstance {
        None,
        Empty(EmptyInstance),
        Mesh(MeshInstance),
    }

    let mut entities = vec![Entity::null(); model.node_count];
    commands.entities.create_empty(&mut entities);

    fn traverse(
        parent: Option<Parent>,
        node: &Node,
        entities: &[Entity],
        asset: &ModelAsset,
        commands: &Commands,
        assets: &Assets,
        res: &mut Vec<ObjectInstance>,
    ) -> Entity {
        let id = res.len();
        res.push(ObjectInstance::None);
        let our_entity = entities[id];
        let us = Some(Parent(our_entity));

        let children = node
            .children
            .iter()
            .map(|child| traverse(us, child, entities, asset, commands, assets, res))
            .collect();

        let model = node.model;
        let position = Position(model.position());
        let rotation = Rotation(model.rotation());
        let scale = Scale(model.scale());
        let mut children = Children(children);

        match &node.data {
            NodeData::Empty => {
                res[id] = ObjectInstance::Empty(EmptyInstance {
                    model,
                    position,
                    rotation,
                    scale,
                    children,
                    parent,
                    name: if node.name.is_empty() {
                        None
                    } else {
                        Some(Name(node.name.clone()))
                    },
                });
            }
            NodeData::MeshGroup(mesh_group) => {
                let mesh_group = &asset.mesh_groups[*mesh_group];
                assert!(!mesh_group.0.is_empty());

                if mesh_group.0.len() == 1 {
                    let mesh_instance = &mesh_group.0[0];
                    let mesh = asset.meshes[mesh_instance.mesh as usize].clone();
                    let material = asset.materials[mesh_instance.material as usize].clone();

                    res[id] = ObjectInstance::Mesh(MeshInstance {
                        mesh: MeshHandle(Some(mesh)),
                        material: MaterialHandle(Some(material)),
                        flags: RenderFlags::SHADOW_CASTER,
                        model,
                        position,
                        rotation,
                        scale,
                        children,
                        parent,
                        name: if node.name.is_empty() {
                            None
                        } else {
                            Some(Name(node.name.clone()))
                        },
                    });
                } else {
                    let mut mesh_instances = (
                        Vec::with_capacity(mesh_group.0.len()),
                        Vec::with_capacity(mesh_group.0.len()),
                        vec![Model(Mat4::IDENTITY); mesh_group.0.len()],
                        vec![Position::default(); mesh_group.0.len()],
                        vec![Rotation::default(); mesh_group.0.len()],
                        vec![Scale::default(); mesh_group.0.len()],
                        vec![RenderFlags::SHADOW_CASTER; mesh_group.0.len()],
                        vec![Children::default(); mesh_group.0.len()],
                        vec![Parent(our_entity); mesh_group.0.len()],
                        (0..mesh_group.0.len())
                            .into_iter()
                            .map(|i| Name(format!("mesh.{i}")))
                            .collect::<Vec<_>>(),
                    );

                    mesh_group.0.iter().for_each(|mesh_instance| {
                        let mesh = asset.meshes[mesh_instance.mesh as usize].clone();
                        let material = asset.materials[mesh_instance.material as usize].clone();
                        mesh_instances.0.push(MeshHandle(Some(mesh)));
                        mesh_instances.1.push(MaterialHandle(Some(material)));
                    });

                    let mut entities = vec![Entity::null(); mesh_group.0.len()];
                    commands.entities.create(mesh_instances, &mut entities);
                    children.0.extend_from_slice(&entities);

                    res[id] = ObjectInstance::Empty(EmptyInstance {
                        model,
                        position,
                        rotation,
                        scale,
                        children,
                        parent,
                        name: if node.name.is_empty() {
                            None
                        } else {
                            Some(Name(node.name.clone()))
                        },
                    });
                }
            }
        }

        our_entity
    }

    let mut roots = Vec::default();
    let mut res = Vec::with_capacity(model.node_count);
    model.roots.iter().for_each(|node| {
        let e = traverse(None, node, &entities, model, commands, assets, &mut res);
        roots.push(e);
    });

    res.into_iter().enumerate().for_each(|(i, obj)| {
        let entity = [entities[i]];
        match obj {
            ObjectInstance::None => unreachable!("should all be some"),
            ObjectInstance::Empty(obj) => {
                commands.entities.set_components(
                    &entity,
                    (
                        Some(obj.children),
                        Some(obj.model),
                        obj.parent,
                        obj.name,
                        Some(obj.position),
                        Some(obj.rotation),
                        Some(obj.scale),
                    ),
                );
            }
            ObjectInstance::Mesh(obj) => {
                commands.entities.set_components(
                    &entity,
                    (
                        Some(obj.mesh),
                        Some(obj.material),
                        Some(obj.flags),
                        Some(obj.children),
                        Some(obj.model),
                        obj.parent,
                        obj.name,
                        Some(obj.position),
                        Some(obj.rotation),
                        Some(obj.scale),
                    ),
                );
            }
        }
    });

    roots
}
