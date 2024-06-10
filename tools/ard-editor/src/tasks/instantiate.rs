use anyhow::Result;
use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::components::transform::{Children, Parent, Position, Rotation, Scale},
    log::warn,
    math::Mat4,
    render::{
        model::{ModelAsset, Node, NodeData},
        prelude::*,
        MaterialInstance, Mesh, Model, RenderFlags,
    },
};

use crate::{
    assets::meta::{MetaData, MetaFile},
    scene_graph::SceneGraph,
};

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
                let handle = self.assets.load::<ModelAsset>(&self.asset.baked);
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
        res: &Res<Everything>,
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
                let roots = instantiate(&model, commands, &self.assets);
                res.get_mut::<SceneGraph>()
                    .unwrap()
                    .add_roots(roots.into_iter());
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
    }

    struct MeshInstance {
        mesh: Mesh,
        material: MaterialInstance,
        render_mode: RenderingMode,
        flags: RenderFlags,
        model: Model,
        position: Position,
        rotation: Rotation,
        scale: Scale,
        children: Children,
        parent: Option<Parent>,
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
                });
            }
            NodeData::MeshGroup(mesh_group) => {
                let mesh_group = &asset.mesh_groups[*mesh_group];
                assert!(!mesh_group.0.is_empty());

                if mesh_group.0.len() == 1 {
                    let mesh_instance = &mesh_group.0[0];
                    let mesh = assets
                        .get(&asset.meshes[mesh_instance.mesh as usize])
                        .unwrap()
                        .mesh
                        .clone();
                    let material = assets
                        .get(&asset.materials[mesh_instance.material as usize])
                        .unwrap();

                    res[id] = ObjectInstance::Mesh(MeshInstance {
                        mesh,
                        material: material.instance.clone(),
                        render_mode: material.render_mode,
                        flags: RenderFlags::SHADOW_CASTER,
                        model,
                        position,
                        rotation,
                        scale,
                        children,
                        parent,
                    });
                } else {
                    let mut mesh_instances = (
                        Vec::with_capacity(mesh_group.0.len()),
                        Vec::with_capacity(mesh_group.0.len()),
                        Vec::with_capacity(mesh_group.0.len()),
                        vec![Model(Mat4::IDENTITY); mesh_group.0.len()],
                        vec![Position::default(); mesh_group.0.len()],
                        vec![Rotation::default(); mesh_group.0.len()],
                        vec![Scale::default(); mesh_group.0.len()],
                        vec![RenderFlags::SHADOW_CASTER; mesh_group.0.len()],
                        vec![Children::default(); mesh_group.0.len()],
                        vec![Parent(our_entity); mesh_group.0.len()],
                    );

                    mesh_group.0.iter().for_each(|mesh_instance| {
                        let mesh = assets
                            .get(&asset.meshes[mesh_instance.mesh as usize])
                            .unwrap()
                            .mesh
                            .clone();
                        let material = assets
                            .get(&asset.materials[mesh_instance.material as usize])
                            .unwrap();

                        mesh_instances.0.push(mesh);
                        mesh_instances.1.push(material.instance.clone());
                        mesh_instances.2.push(material.render_mode);
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
                        Some(obj.render_mode),
                        Some(obj.flags),
                        Some(obj.children),
                        Some(obj.model),
                        obj.parent,
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
