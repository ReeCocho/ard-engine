use ard_engine::{
    ecs::prelude::*,
    math::{Vec4, Vec4Swizzles},
    render::{shape::Shape, DebugDraw, DebugDrawing, Mesh, Model},
};

use crate::{
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

        if let Some(query) = ctx.queries.get::<(Read<Model>, Read<Mesh>)>(entity) {
            let (model, mesh) = *query;
            let bounds = mesh.bounds();
            ctx.res.get_mut::<DebugDrawing>().unwrap().draw(DebugDraw {
                color: Vec4::new(1.0, 1.0, 0.0, 1.0),
                shape: Shape::Box {
                    min_pt: bounds.min_pt.xyz(),
                    max_pt: bounds.max_pt.xyz(),
                    model: model.0,
                },
            });
        }

        self.inspectors
            .show(ctx.ui, entity, ctx.commands, ctx.queries, ctx.res);
    }
}
