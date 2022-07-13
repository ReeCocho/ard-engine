use ard_engine::{
    assets::prelude::Assets,
    ecs::prelude::*,
    game::{SceneDescriptor, SceneGameObject},
};

use crate::{editor_job::EditorJobQueue, scene_graph::SceneGraph};

use super::dirty_assets::DirtyAssets;

#[derive(Default)]
pub struct ToolBar {}

impl ToolBar {
    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        queries: &Queries<Everything>,
        scene_graph: &mut SceneGraph,
        assets: &Assets,
        commands: &EntityCommands,
        dirty: &mut DirtyAssets,
        jobs: &mut EditorJobQueue,
    ) {
        ui.main_menu_bar(|| {
            ui.menu("File", || {
                if ui.menu_item("Save") {
                    jobs.add(dirty.flush(assets));

                    // let descriptor = scene_graph.save(queries, assets);
                    // println!("{}", ron::to_string(&descriptor).unwrap());
                }
            });

            ui.menu("New", || {
                if ui.menu_item("Static Object") {
                    scene_graph.create(SceneGameObject::StaticObject, commands);
                }
            });
        });
    }
}
