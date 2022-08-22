use ard_engine::{ecs::prelude::*, game::components::transform::Parent};

use crate::{
    controller::Command, scene_graph::SceneGraphNode, util::ui::DragDropPayload,
    view::inspector::InspectorItem,
};

use super::View;

#[derive(Default)]
pub struct Hierarchy {}

enum HierarchyCommand {
    Reparent {
        entity: Entity,
        new_parent: Option<Entity>,
    },
    Destroy {
        entity: Entity,
    },
}

struct Reparent {
    entity: Entity,
    old_parent: Option<Entity>,
    new_parent: Option<Entity>,
}

struct Destroy {
    entity: Entity,
    parent: Option<Entity>,
    node: Option<SceneGraphNode>,
}

impl View for Hierarchy {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        controller: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
    ) {
        ui.window("Hierarchy").build(|| {
            // First entity is the entity to reparent and second is the new parent
            fn build_tree(
                ui: &imgui::Ui,
                events: &Events,
                node: &SceneGraphNode,
            ) -> Option<HierarchyCommand> {
                let mut command = None;

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
                        if let DragDropPayload::Entity(entity) = payload_data.data {
                            command = Some(HierarchyCommand::Reparent {
                                entity,
                                new_parent: Some(node.entity),
                            });
                        }
                    }

                    target.pop();
                }

                // Context menu for entities
                if ui.is_item_hovered() && ui.is_mouse_released(imgui::MouseButton::Right) {
                    ui.open_popup("Entity Context Menu");
                }

                if let Some(popup) = ui.begin_popup("Entity Context Menu") {
                    if ui.menu_item("Destroy") {
                        command = Some(HierarchyCommand::Destroy {
                            entity: node.entity,
                        });
                    }
                    popup.end()
                }

                // If the node is expanded, draw children
                if let Some(tree_node) = tree_node {
                    for child in &node.children {
                        let res = build_tree(ui, events, child);
                        if res.is_some() {
                            command = res;
                        }
                    }
                    tree_node.pop();
                }

                command
            }

            // Build the tree view
            let mut command = None;
            ui.group(|| {
                // Drag and drop onto the root of the hierarchy
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload_data)) = target.accept_payload::<DragDropPayload, _>(
                        "Entity",
                        imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID,
                    ) {
                        if let DragDropPayload::Entity(entity) = payload_data.data {
                            command = Some(HierarchyCommand::Reparent {
                                entity,
                                new_parent: None,
                            });
                        }
                    }

                    target.pop();
                }

                for root in resc.scene_graph.roots() {
                    let res = build_tree(ui, &resc.ecs_commands.events, root);
                    if res.is_some() {
                        command = res;
                    }
                }
            });

            // Perform command if requested
            if let Some(command) = command {
                match command {
                    HierarchyCommand::Reparent { entity, new_parent } => {
                        let old_parent =
                            resc.queries.get::<Read<Parent>>(entity).unwrap().0.clone();
                        controller.submit(Reparent {
                            entity,
                            new_parent,
                            old_parent,
                        });
                    }
                    HierarchyCommand::Destroy { entity } => {
                        let parent = resc.queries.get::<Read<Parent>>(entity).unwrap().0.clone();
                        controller.submit(Destroy {
                            entity,
                            parent,
                            node: None,
                        });
                    }
                }
            }
        });
    }
}

impl Command for Reparent {
    #[inline]
    fn undo(&mut self, resc: &mut crate::editor::Resources) {
        resc.scene_graph.set_parent(
            self.entity,
            self.old_parent,
            &resc.queries,
            &resc.ecs_commands,
        );
    }

    #[inline]
    fn redo(&mut self, resc: &mut crate::editor::Resources) {
        resc.scene_graph.set_parent(
            self.entity,
            self.new_parent,
            &resc.queries,
            &resc.ecs_commands,
        );
    }
}

impl Command for Destroy {
    fn undo(&mut self, resc: &mut crate::editor::Resources) {
        // Reenable the entity
        crate::util::enable(self.entity, &resc.queries, &resc.ecs_commands.entities);

        // Put the entity back into the scene graph with the appropriate parent
        if let Some(node) = self.node.take() {
            resc.scene_graph.add_node(node);
            resc.scene_graph.set_parent(
                self.entity,
                self.parent.clone(),
                &resc.queries,
                &resc.ecs_commands,
            );
        }
    }

    fn redo(&mut self, resc: &mut crate::editor::Resources) {
        // Disable the entity so it is "effectively" destroyed
        crate::util::disable(self.entity, &resc.queries, &resc.ecs_commands.entities);

        // Remove it from the scene graph so it is not visible
        self.node = resc.scene_graph.remove_entity(self.entity);
    }
}
