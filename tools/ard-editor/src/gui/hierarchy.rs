use crate::{
    command::{
        entity::{CreateEmptyEntity, DestroyEntity, SetParentCommand},
        EditorCommands,
    },
    scene_graph::SceneGraph,
    selected::Selected,
};
use ard_engine::{core::core::Name, ecs::prelude::*, transform::Children};

use super::{drag_drop::DragDropPayload, util, EditorViewContext};

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
        let scene_graph = ctx.res.get::<SceneGraph>().unwrap();
        let selected = match *ctx.res.get::<Selected>().unwrap() {
            Selected::Entity(e) => Some(e),
            _ => None,
        };

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ctx.ui, |ui| {
                let ctx = EditorViewContext {
                    tick: ctx.tick,
                    ui,
                    commands: ctx.commands,
                    queries: ctx.queries,
                    res: ctx.res,
                };
                self.show_entities(None, scene_graph.roots(), selected, ctx);
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

    fn show_entities(
        &mut self,
        mut parent: Option<Entity>,
        entities: &[Entity],
        selected: Option<Entity>,
        ctx: EditorViewContext,
    ) {
        let frame = egui::Frame::none();
        let mut index = 0;
        let (_, payload) = util::hidden_drop_zone::<DragDropPayload, _>(ctx.ui, frame, |ui| {
            entities.iter().enumerate().for_each(|(i, entity)| {
                let ctx = EditorViewContext {
                    tick: ctx.tick,
                    ui,
                    commands: ctx.commands,
                    queries: ctx.queries,
                    res: ctx.res,
                };

                let response = match self.show_entity(*entity, selected, ctx) {
                    Some(response) => response,
                    None => return,
                };

                if let (Some(pointer), Some(payload)) = (
                    ui.input(|i| i.pointer.interact_pos()),
                    response.dnd_hover_payload::<DragDropPayload>(),
                ) {
                    let rect = response.rect;
                    let stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

                    let _payload_entity = match *payload {
                        DragDropPayload::Entity(entity) => entity,
                        _ => return,
                    };

                    // Above us
                    if pointer.y < rect.center().y - (rect.height() * 0.166) {
                        ui.painter().hline(rect.x_range(), rect.top(), stroke);
                        index = i;
                    }
                    // Below us
                    else if pointer.y > rect.center().y + (rect.height() * 0.166) {
                        ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                        index = i + 1;
                    }
                    // On top of us
                    else {
                        ui.painter().hline(rect.x_range(), rect.center().y, stroke);
                        index = 0;
                        parent = Some(*entity);
                    }
                }
            });
        });

        if let Some(payload) = payload {
            if let DragDropPayload::Entity(entity) = *payload {
                ctx.res
                    .get_mut::<EditorCommands>()
                    .unwrap()
                    .submit(SetParentCommand::new(
                        entity,
                        parent,
                        index,
                        ctx.queries,
                        ctx.res,
                    ));
            }
        }
    }

    fn show_entity(
        &mut self,
        entity: Entity,
        selected: Option<Entity>,
        ctx: EditorViewContext,
    ) -> Option<egui::Response> {
        let children = match ctx.queries.get::<Read<Children>>(entity) {
            Some(children) => children,
            None => return None,
        };

        let name = match ctx.queries.get::<Read<Name>>(entity) {
            Some(name) => name.0.clone(),
            None => format!("(Entity {})", entity.id()),
        };

        let id = ctx.ui.make_persistent_id(entity);
        let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ctx.ui.ctx(),
            id,
            false,
        );

        let has_children = !children.0.is_empty();
        let header_res = ctx.ui.horizontal(|ui| {
            header.show_toggle_button(ui, move |ui, openness, response| {
                if has_children {
                    egui::collapsing_header::paint_default_icon(ui, openness, response)
                }
            });
            ui.dnd_drag_source(
                egui::Id::new(format!("drag_{entity:?}")),
                DragDropPayload::Entity(entity),
                |ui| {
                    ui.add({
                        let mut text = egui::RichText::new(name);
                        if selected == Some(entity) {
                            text = text.strong();
                        }
                        egui::Label::new(text)
                            .selectable(false)
                            .sense(egui::Sense::click_and_drag())
                    })
                },
            )
            .response
        });

        header.show_body_indented(&header_res.response, ctx.ui, |ui| {
            let ctx = EditorViewContext {
                tick: ctx.tick,
                ui,
                commands: ctx.commands,
                queries: ctx.queries,
                res: ctx.res,
            };
            self.show_entities(Some(entity), &children.0, selected, ctx)
        });

        let response = header_res.inner;

        if let Some(rename) = self.rename.as_ref() {
            if rename.entity == entity && !response.context_menu_opened() {
                self.rename = None;
            }
        }

        let click_resp = response.interact(egui::Sense::click());
        if click_resp.clicked() || click_resp.secondary_clicked() {
            let mut selected = ctx.res.get_mut::<Selected>().unwrap();
            *selected = Selected::Entity(entity);
        }

        response.context_menu(|ui| {
            if ui.button("Destroy").clicked() {
                ctx.res
                    .get_mut::<EditorCommands>()
                    .unwrap()
                    .submit(DestroyEntity::new(entity));
            }

            if let Some(rename) = self.rename.take() {
                self.rename = rename.show(ui, ctx.queries);
            } else if ui.button("Rename").clicked() {
                self.rename = Some(Rename {
                    entity,
                    new_name: ctx
                        .queries
                        .get::<Read<Name>>(entity)
                        .map(|n| n.0.clone())
                        .unwrap_or_default(),
                })
            }
        });

        Some(response)
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
