use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::{
        components::{
            renderable::{RenderableData, RenderableSource},
            transform::{Children, Parent, Transform},
        },
        object::{empty::EmptyObject, static_object::StaticObject},
        SceneGameObject,
    },
    graphics::prelude::*,
    graphics_assets::prelude::ModelAsset,
    math::*,
};
use smallvec::SmallVec;

use crate::scene_graph::SceneGraphNode;

pub mod asset_meta;
pub mod dirty_assets;
pub mod editor_job;
pub mod par_task;
pub mod ui;

/// Constructs an empty entity as a root and then parents all the entities of a model to that root.
pub fn instantiate_model(
    model: &ModelAsset,
    handle: &Handle<ModelAsset>,
    commands: &EntityCommands,
) -> SceneGraphNode {
    // Create the empty root
    let mut empty = [Entity::null()];
    commands.create_empty(&mut empty);

    let mut graph_node = SceneGraphNode {
        entity: empty[0],
        children: Vec::default(),
        ty: SceneGameObject::EmptyObject,
    };

    // Create the children objects
    let mut empty_children = SmallVec::default();

    for node in &model.nodes {
        let meshes = &model.mesh_groups[node.mesh_group].meshes;

        let scale = Vec3::new(
            node.transform.col(0).xyz().length(),
            node.transform.col(1).xyz().length(),
            node.transform.col(2).xyz().length(),
        );

        let mut rot_mat = node.transform;
        *rot_mat.col_mut(0) /= scale.x;
        *rot_mat.col_mut(1) /= scale.y;
        *rot_mat.col_mut(2) /= scale.z;

        let transform = Transform {
            position: node.transform.col(3).xyz().into(),
            rotation: Quat::from_mat4(&rot_mat),
            scale: scale.into(),
        };

        let mut children = StaticObject {
            field_Transform: vec![transform; meshes.len()],
            field_Parent: vec![Parent(Some(empty[0])); meshes.len()],
            field_Children: vec![Children::default(); meshes.len()],
            field_Model: vec![Model(node.transform); meshes.len()],
            field_RenderableData: Vec::with_capacity(meshes.len()),
        };

        for i in 0..meshes.len() {
            children.field_RenderableData.push(RenderableData {
                source: Some(RenderableSource::Model {
                    model: handle.clone(),
                    mesh_group_idx: node.mesh_group,
                    mesh_idx: i,
                }),
            });
        }

        let mut entities = vec![Entity::null(); meshes.len()];
        commands.create(
            (
                children.field_Transform,
                children.field_Parent,
                children.field_Children,
                children.field_Model,
                children.field_RenderableData,
            ),
            &mut entities,
        );

        for entity in &entities {
            graph_node.children.push(SceneGraphNode {
                entity: *entity,
                children: Vec::default(),
                ty: SceneGameObject::StaticObject,
            });
        }

        empty_children.extend(entities.into_iter());
    }

    // Set the components of the empty root
    let empty_object = EmptyObject {
        field_Parent: vec![Parent::default()],
        field_Transform: vec![Transform::default()],
        field_Children: vec![Children(empty_children)],
        field_Model: vec![Model::default()],
    };

    commands.set_components(
        &empty,
        (
            empty_object.field_Parent,
            empty_object.field_Transform,
            empty_object.field_Children,
            empty_object.field_Model,
        ),
    );

    graph_node
}
