use ard_engine::ecs::prelude::Events;

use crate::{
    gui::{inspector::InspectorItem, util::DragDropPayload},
    scene_graph::{SceneGraph, SceneGraphNode},
};

#[derive(Default)]
pub struct Hierarchy {}

impl Hierarchy {
    pub fn draw(&mut self, ui: &imgui::Ui, events: &Events, scene_graph: &SceneGraph) {
        ui.window("Hierarchy").build(|| {
            fn build_tree(ui: &imgui::Ui, events: &Events, node: &SceneGraphNode) {
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

                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload_data)) = target.accept_payload::<DragDropPayload, _>(
                        "Entity",
                        imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID,
                    ) {
                        println!("{} {:?}", &name, payload_data);
                    }

                    target.pop();
                }

                // If the node is expanded, draw children
                if let Some(tree_node) = tree_node {
                    for child in &node.children {
                        build_tree(ui, events, child);
                    }
                    tree_node.pop();
                }
            }

            for root in scene_graph.roots() {
                build_tree(ui, events, root);
            }
        });
    }
}
