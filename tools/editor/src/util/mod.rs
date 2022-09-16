use ard_engine::{
    assets::prelude::*,
    core::prelude::*,
    ecs::prelude::*,
    game::{
        components::{
            renderable::{RenderableData, RenderableSource},
            transform::{Children, Parent, Transform},
        },
        destroy::Destroy,
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

/// Disables an entity and all of it's children.
pub fn disable(entity: Entity, queries: &Queries<Everything>, commands: &EntityCommands) {
    let mut to_disable = vec![entity];
    let mut i = 0;
    while i != to_disable.len() {
        let entity = to_disable[i];
        commands.add_tag(entity, Disabled);
        if let Some(children) = queries.get::<Read<Children>>(entity) {
            for child in children.0.iter() {
                to_disable.push(*child);
            }
        };
        i += 1;
    }
}

/// Enables an entity and all of it's children.
pub fn enable(entity: Entity, queries: &Queries<Everything>, commands: &EntityCommands) {
    let mut to_enable = vec![entity];
    let mut i = 0;
    while i != to_enable.len() {
        let entity = to_enable[i];
        commands.remove_tag::<Disabled>(entity);
        if let Some(children) = queries.get::<Read<Children>>(entity) {
            for child in children.0.iter() {
                to_enable.push(*child);
            }
        };
        i += 1;
    }
}

/// Destroys an entity and all of it's children.
pub fn destroy(entity: Entity, queries: &Queries<Everything>, commands: &EntityCommands) {
    let mut to_destroy = vec![entity];
    let mut i = 0;
    while i != to_destroy.len() {
        let entity = to_destroy[i];
        commands.add_component(entity, Destroy);
        if let Some(children) = queries.get::<Read<Children>>(entity) {
            for child in children.0.iter() {
                to_destroy.push(*child);
            }
        };
        i += 1;
    }
}

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

/// Extracts the translation, rotation (as a matrix), and scale from a model matrix.
pub fn extract_transformations(model: Mat4) -> (Vec3, Mat4, Vec3) {
    let pos = model.col(3).xyz();

    let scale = Vec3::new(
        model.col(0).xyz().length() * model.col(0).x.signum(),
        model.col(1).xyz().length() * model.col(1).y.signum(),
        model.col(2).xyz().length() * model.col(2).z.signum(),
    );

    let rot = Mat4::from_cols(
        if scale.x == 0.0 {
            Vec4::X
        } else {
            Vec4::from((model.col(0).xyz() / scale.x, 0.0))
        },
        if scale.y == 0.0 {
            Vec4::Y
        } else {
            Vec4::from((model.col(1).xyz() / scale.y, 0.0))
        },
        if scale.z == 0.0 {
            Vec4::Z
        } else {
            Vec4::from((model.col(2).xyz() / scale.z, 0.0))
        },
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );

    (pos, rot, scale)
}
