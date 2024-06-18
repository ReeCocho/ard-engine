use anyhow::Result;
use ard_engine::{
    assets::prelude::*,
    core::prelude::*,
    ecs::prelude::*,
    game::components::transform::{Children, Parent, Position, Rotation, Scale},
    log::warn,
    math::Mat4,
    render::{
        loader::{MaterialHandle, MeshHandle},
        model::{ModelAsset, Node, NodeData},
        Model, RenderFlags,
    },
};

use crate::assets::meta::{MetaData, MetaFile};

use super::{EditorTask, TaskConfirmation};

pub struct InstantiateTask {
    asset: MetaFile,
    assets: Assets,
    handle: Option<InstantiateAssetHandle>,
}

enum InstantiateAssetHandle {
    Model(Handle<ModelAsset>),
}

impl InstantiateTask {
    pub fn new(asset: MetaFile, assets: Assets) -> Self {
        Self {
            asset,
            assets,
            handle: None,
        }
    }
}

impl EditorTask for InstantiateTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        unreachable!()
    }

    fn run(&mut self) -> anyhow::Result<()> {
        match &self.asset.data {
            MetaData::Model => {
                let handle = match self.assets.load::<ModelAsset>(&self.asset.baked) {
                    Some(handle) => handle,
                    None => {
                        return Err(anyhow::Error::msg(format!(
                            "could not load {:?}",
                            self.asset.raw
                        )))
                    }
                };
                self.assets.wait_for_load(&handle);
                self.handle = Some(InstantiateAssetHandle::Model(handle));
            }
        }

        Ok(())
    }

    fn complete(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) -> Result<()> {
        let handle = match self.handle.take() {
            Some(handle) => handle,
            None => {
                warn!("Finished loading asset, but did not get a handle back.");
                return Ok(());
            }
        };

        match handle {
            InstantiateAssetHandle::Model(handle) => {
                let model = self.assets.get(&handle).unwrap();
                instantiate(&model, commands, &self.assets);
            }
        }

        Ok(())
    }
}

fn instantiate(model: &ModelAsset, commands: &Commands, assets: &Assets) -> Vec<Entity> {
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
