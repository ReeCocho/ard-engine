use ard_engine::ecs::prelude::*;

use crate::{
    gui::{inspector::InspectorItem, util::DragDropPayload},
    scene_graph::{SceneGraph, SceneGraphNode},
};

#[derive(Default)]
pub struct Hierarchy {}

impl Hierarchy {
    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        events: &Events,
        scene_graph: &mut SceneGraph,
        queries: &Queries<Everything>,
        commands: &Commands,
    ) {
        ui.window("Hierarchy").build(|| {
            // First entity is the entity to reparent and second is the new parent
            fn build_tree(
                ui: &imgui::Ui,
                events: &Events,
                node: &SceneGraphNode,
            ) -> Option<(Entity, Option<Entity>)> {
                let name = format!("Entity {}", node.entity.id());

                let tree_node = ui
                    .tree_node_config(&name)
                    .open_on_arrow(true)
                    .leaf(node.children.is_empty())
                    .push();

                // Select the entity in the inspector
                if ui.is_item_clicked() {
                    events.submit(InspectorItem::Entity(node.entity));
                }

                // Drag/drop for entities
                if let Some(tooltip) = ui
                    .drag_drop_source_config("Entity")
                    .flags(imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID)
                    .begin_payload(DragDropPayload::Entity(node.entity))
                {
                    ui.text(&name);
                    tooltip.end();
                }

                let mut reparent = None;
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload_data)) = target.accept_payload::<DragDropPayload, _>(
                        "Entity",
                        imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID,
                    ) {
                        if let DragDropPayload::Entity(entity) = payload_data.data {
                            reparent = Some((entity, Some(node.entity)));
                        }
                    }

                    target.pop();
                }

                // If the node is expanded, draw children
                if let Some(tree_node) = tree_node {
                    for child in &node.children {
                        let res = build_tree(ui, events, child);
                        if res.is_some() {
                            reparent = res;
                        }
                    }
                    tree_node.pop();
                }

                reparent
            }

            // Build the tree view
            let mut reparent = None;
            ui.group(|| {
                // Drag and drop onto the root of the hierarchy
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload_data)) = target.accept_payload::<DragDropPayload, _>(
                        "Entity",
                        imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID,
                    ) {
                        if let DragDropPayload::Entity(entity) = payload_data.data {
                            reparent = Some((entity, None));
                        }
                    }

                    target.pop();
                }

                for root in scene_graph.roots() {
                    let res = build_tree(ui, events, root);
                    if res.is_some() {
                        reparent = res;
                    }
                }
            });

            // Reparent if requested
            if let Some((entity, new_parent)) = reparent {
                scene_graph.set_parent(entity, new_parent, queries, commands);
            }
        });
    }
}
