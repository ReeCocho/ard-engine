use ard_engine::{
    assets::prelude::{Handle, RawHandle},
    ecs::prelude::Entity,
    ecs::prelude::EntityCommands,
    game::{
        components::{
            renderable::{RenderableData, RenderableSource},
            transform::{Children, Parent, Transform},
        },
        object::{empty::EmptyObject, static_object::StaticObject},
        SceneGameObject,
    },
    graphics::prelude::Model,
    graphics_assets::prelude::ModelAsset,
    math::{Quat, Vec3, Vec4Swizzles},
};
use smallvec::SmallVec;

use crate::scene_graph::SceneGraphNode;

/// This should be the only type used for drag and drop. Add items as neccesary.
#[derive(Debug, Copy, Clone)]
pub enum DragDropPayload {
    Entity(Entity),
    Asset(RawHandle),
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

pub fn throbber(
    ui: &imgui::Ui,
    radius: f32,
    thickness: f32,
    num_segments: i32,
    speed: f32,
    color: impl Into<imgui::ImColor32>,
) {
    let mut pos = ui.cursor_pos();
    let wpos = ui.window_pos();
    pos[0] += wpos[0];
    pos[1] += wpos[1];

    let size = [radius * 2.0, radius * 2.0];

    let rect = imgui::sys::ImRect {
        Min: imgui::sys::ImVec2::new(pos[0] - thickness, pos[1] - thickness),
        Max: imgui::sys::ImVec2::new(pos[0] + size[0] + thickness, pos[1] + size[1] + thickness),
    };

    unsafe {
        imgui::sys::igItemSizeRect(rect, 0.0);

        if !imgui::sys::igItemAdd(
            rect,
            0,
            std::ptr::null(),
            imgui::sys::ImGuiItemFlags_None as i32,
        ) {
            return;
        }
    }

    let time = ui.time() as f32 * speed;

    let start = (time.sin() * (num_segments - 5) as f32).abs() as i32;
    let min = 2.0 * std::f32::consts::PI * (start as f32 / num_segments as f32);
    let max = 2.0 * std::f32::consts::PI * ((num_segments - 3) as f32 / num_segments as f32);
    let center = [pos[0] + radius, pos[1] + radius];

    let mut points = Vec::with_capacity(num_segments as usize);

    for i in 0..num_segments {
        let a = min + (i as f32 / num_segments as f32) * (max - min);
        let x = (a + time * 8.0).cos() * radius;
        let y = (a + time * 8.0).sin() * radius;
        let new_pos = [center[0] + x, center[1] + y];

        points.push(new_pos);
    }

    // NOTE: Polyline is supposed to be in window coordinates, but for whatever reason it is
    // actually in screen coordinates here. If the throbber ever bugs out, check this first.
    ui.get_window_draw_list()
        .add_polyline(points, color.into())
        .thickness(thickness)
        .build();
}
