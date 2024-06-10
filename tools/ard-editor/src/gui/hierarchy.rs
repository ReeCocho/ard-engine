use crate::scene_graph::SceneGraph;
use ard_engine::{core::core::Tick, ecs::prelude::*, game::components::transform::Children};

use super::EditorViewContext;

pub struct HierarchyView {}

impl HierarchyView {
    pub fn show(&mut self, mut ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let scene_graph = ctx.res.get::<SceneGraph>().unwrap();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ctx.ui, |ui| {
                scene_graph.roots().iter().for_each(|root| {
                    Self::show_entity(*root, ui, ctx.tick, ctx.commands, ctx.queries, ctx.res);
                });
            });
        egui_tiles::UiResponse::None
    }

    fn show_entity(
        entity: Entity,
        ui: &mut egui::Ui,
        tick: Tick,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        ui.collapsing(format!("Entity {}", entity.id()), |ui| {
            let children = match queries.get::<Read<Children>>(entity) {
                Some(children) => children,
                None => return,
            };

            children.0.iter().for_each(|child| {
                Self::show_entity(*child, ui, tick, commands, queries, res);
            });
        });
    }
}
