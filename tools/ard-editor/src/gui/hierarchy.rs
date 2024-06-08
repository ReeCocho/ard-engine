use crate::scene_graph::SceneGraph;
use ard_engine::{ecs::prelude::*, game::components::transform::Children};

use super::EditorViewContext;

pub struct HierarchyView {

}

impl HierarchyView {
    pub fn show(&mut self, mut ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let scene_graph = ctx.res.get::<SceneGraph>().unwrap();

        scene_graph.roots().iter().for_each(|root| {
            Self::show_entity(*root, &mut ctx);
        });

        egui_tiles::UiResponse::None 
    }

    fn show_entity(entity: Entity, ctx: &mut EditorViewContext) {
        ctx.ui.collapsing(format!("Entity {}", entity.id()), |ui| {
            let children = match ctx.queries.get::<Read<Children>>(entity) {
                Some(children) => children,
                None => return,
            };

            children.0.iter().for_each(|child| {
                Self::show_entity(*child, &mut EditorViewContext {
                    ui,
                    tick: ctx.tick,
                    commands: ctx.commands,
                    queries: ctx.queries,
                    res: ctx.res
                });
            });
        });
    }
}