use ard_engine::{ecs::prelude::*, game::components::destroy::Destroy};

use crate::selected::Selected;

use super::EditorViewContext;

pub struct InspectorView {}

impl InspectorView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut selected = ctx.res.get_mut::<Selected>().unwrap();

        match *selected {
            Selected::None => {
                ctx.ui.label("Nothing");
            }
            Selected::Entity(e) => self.inspect_entity(ctx, &mut selected, e),
        };

        egui_tiles::UiResponse::None
    }

    fn inspect_entity(&mut self, ctx: EditorViewContext, selected: &mut Selected, entity: Entity) {
        if !ctx.queries.is_alive(entity) {
            *selected = Selected::None;
            return;
        }

        if ctx.ui.button("Delete").clicked() {
            ctx.commands.entities.add_component(entity, Destroy);
        }
    }
}
