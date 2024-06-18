use crate::{
    command::{
        entity::{CreateEmptyEntity, DestroyEntity},
        EditorCommands,
    },
    scene_graph::SceneGraph,
    selected::Selected,
};
use ard_engine::{
    core::core::{Name, Tick},
    ecs::prelude::*,
    game::components::transform::Children,
};

use super::EditorViewContext;

#[derive(Default)]
pub struct HierarchyView {
    rename: Option<Rename>,
}

struct Rename {
    entity: Entity,
    new_name: String,
}

impl HierarchyView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut scene_graph = ctx.res.get_mut::<SceneGraph>().unwrap();
        let selected = match *ctx.res.get::<Selected>().unwrap() {
            Selected::Entity(e) => Some(e),
            _ => None,
        };

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ctx.ui, |ui| {
                scene_graph.roots_mut().retain(|root| {
                    self.show_entity(
                        *root,
                        selected,
                        ui,
                        ctx.tick,
                        ctx.commands,
                        ctx.queries,
                        ctx.res,
                    );
                    ctx.queries.is_alive(*root)
                });
                ui.allocate_response(ui.available_size(), egui::Sense::click())
            })
            .inner
            .context_menu(|ui| {
                if ui.button("Create Empty Entity").clicked() {
                    ctx.res
                        .get_mut::<EditorCommands>()
                        .unwrap()
                        .submit(CreateEmptyEntity::default());
                }
            });
        egui_tiles::UiResponse::None
    }

    fn show_entity(
        &mut self,
        entity: Entity,
        selected: Option<Entity>,
        ui: &mut egui::Ui,
        tick: Tick,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let children = match queries.get::<Read<Children>>(entity) {
            Some(children) => children,
            None => return,
        };

        let name = match queries.get::<Read<Name>>(entity) {
            Some(name) => name.0.clone(),
            None => format!("(Entity {})", entity.id()),
        };

        let response = if children.0.is_empty() {
            egui::CollapsingHeader::new(name)
                .id_source(entity)
                .icon(|_, _, _| {})
                .open(Some(false))
                .show_background(selected == Some(entity))
                .show(ui, |_| {})
                .header_response
        } else {
            egui::CollapsingHeader::new(name)
                .id_source(entity)
                .show_background(selected == Some(entity))
                .show(ui, |ui| {
                    children.0.iter().for_each(|child| {
                        self.show_entity(*child, selected, ui, tick, commands, queries, res);
                    });
                })
                .header_response
        };

        response.context_menu(|ui| {
            if ui.button("Destroy").clicked() {
                res.get_mut::<EditorCommands>()
                    .unwrap()
                    .submit(DestroyEntity::new(entity));
            }

            if let Some(rename) = self.rename.take() {
                self.rename = rename.show(ui, queries);
            } else if ui.button("Rename").clicked() {
                self.rename = Some(Rename {
                    entity,
                    new_name: queries
                        .get::<Read<Name>>(entity)
                        .map(|n| n.0.clone())
                        .unwrap_or_default(),
                })
            }
        });

        if response.clicked() || response.secondary_clicked() {
            let mut selected = res.get_mut::<Selected>().unwrap();
            *selected = Selected::Entity(entity);
        }
    }
}

impl Rename {
    pub fn show(mut self, ui: &mut egui::Ui, queries: &Queries<Everything>) -> Option<Self> {
        let prev = self.new_name.clone();
        let response = egui::TextEdit::singleline(&mut self.new_name)
            .show(ui)
            .response;
        if self.new_name.is_empty() {
            self.new_name = prev;
        }
        queries.get::<Write<Name>>(self.entity).map(|mut name| {
            name.0 = self.new_name.clone();
        });

        if response.lost_focus() {
            None
        } else {
            Some(self)
        }
    }
}
