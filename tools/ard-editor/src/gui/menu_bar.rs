use ard_engine::{core::core::Stop, ecs::prelude::*, render::PathTracerSettings};

use crate::{
    assets::{CurrentAssetPath, EditorAssets},
    scene_graph::SceneGraph,
    tasks::{build::BuildGameTask, save::SaveSceneTask, TaskQueue},
};

pub struct MenuBar;

impl MenuBar {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                let editor_assets = res.get::<EditorAssets>().unwrap();
                let current_path = res.get::<CurrentAssetPath>().unwrap();
                let task_queue = res.get_mut::<TaskQueue>().unwrap();

                if ui.button("Save").clicked() {
                    let scene_graph = res.get::<SceneGraph>().unwrap();
                    task_queue.add(
                        match scene_graph
                            .active_scene()
                            .and_then(|name| editor_assets.find_asset(name))
                        {
                            Some(asset) => SaveSceneTask::new_overwrite(asset),
                            None => SaveSceneTask::new(current_path.path()),
                        },
                    );
                }

                if ui.button("Build").clicked() {
                    task_queue.add(BuildGameTask::default());
                }

                if ui.button("Quit").clicked() {
                    commands.events.submit(Stop);
                }
            });

            ui.menu_button("Tools", |ui| {
                let mut pt = res.get_mut::<PathTracerSettings>().unwrap();

                if ui.button("Toggle Path Tracer").clicked() {
                    pt.enabled = !pt.enabled;
                }
            });
        });
    }
}
