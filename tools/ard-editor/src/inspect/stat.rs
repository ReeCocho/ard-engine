use ard_engine::{
    core::stat::{DirtyStatic, Static},
    ecs::prelude::*,
    game::components::stat::MarkStatic,
    transform::Parent,
};

use crate::scene_graph::SceneGraph;

use super::InspectorContext;

#[derive(Default)]
pub struct StaticInspector;

impl StaticInspector {
    pub fn show(&mut self, ctx: InspectorContext) {
        let mut is_static = ctx.queries.get::<Read<MarkStatic>>(ctx.entity).is_some();
        if !ctx.ui.toggle_value(&mut is_static, "Is Static").changed() {
            return;
        }

        if ctx.queries.get::<Read<Parent>>(ctx.entity).is_some() {
            return;
        }

        let all_children = SceneGraph::collect_children(ctx.queries, vec![ctx.entity]);

        if is_static {
            all_children.into_iter().for_each(|entity| {
                ctx.commands.entities.add_component(entity, MarkStatic);
            });
        } else {
            all_children.into_iter().for_each(|entity| {
                ctx.commands.entities.remove_component::<MarkStatic>(entity);
                ctx.commands.entities.remove_component::<Static>(entity);
                ctx.res.get::<DirtyStatic>().unwrap().signal(0);
            });
        }
    }
}
