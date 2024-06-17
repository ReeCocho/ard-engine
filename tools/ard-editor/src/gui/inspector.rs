use ard_engine::{ecs::prelude::*, render::Model};

use crate::{
    command::{entity::DestroyEntity, EditorCommands},
    inspect::{transform::TransformInspector, Inspectors},
    selected::Selected,
};

use super::EditorViewContext;

pub struct InspectorView {
    inspectors: Inspectors,
}

impl Default for InspectorView {
    fn default() -> Self {
        let mut inspectors = Inspectors::default();
        inspectors.with(TransformInspector);

        Self { inspectors }
    }
}

impl InspectorView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut selected = ctx.res.get_mut::<Selected>().unwrap();

        match *selected {
            Selected::None => {
                ctx.ui.label("Nothing");
            }
            Selected::Entity(e) => {
                if ctx.queries.get::<Read<Model>>(e).is_none() {
                    *selected = Selected::None;
                    return egui_tiles::UiResponse::None;
                }
                self.inspect_entity(ctx, &mut selected, e)
            }
        };

        egui_tiles::UiResponse::None
    }

    fn inspect_entity(&mut self, ctx: EditorViewContext, selected: &mut Selected, entity: Entity) {
        if !ctx.queries.is_alive(entity) {
            *selected = Selected::None;
            return;
        }

        if ctx.ui.button("Delete").clicked() {
            ctx.res
                .get_mut::<EditorCommands>()
                .unwrap()
                .submit(DestroyEntity::new(entity));
        }

        self.inspectors
            .show(ctx.ui, entity, ctx.commands, ctx.queries, ctx.res);
    }
}
