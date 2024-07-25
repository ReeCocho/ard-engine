use ard_engine::{core::core::Stop, ecs::prelude::*};

use crate::{
    assets::{CurrentAssetPath, EditorAssets},
    scene_graph::SceneGraph,
    tasks::{save::SaveSceneTask, TaskQueue},
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
                // let assets = res.get::<Assets>().unwrap();
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

                if ui.button("Make Lof").clicked() {
                    let editor_assets = res.get::<EditorAssets>().unwrap();
                    let manifest = editor_assets.build_manifest();
                    let f = std::fs::File::options()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open("./packages/main/main.manifest")
                        .unwrap();
                    bincode::serialize_into(f, &manifest).unwrap();

                    ard_engine::assets::package::lof::create_lof_from_folder(
                        "./main.lof",
                        "./packages/main/",
                    );

                    std::fs::remove_file("./packages/main/main.manifest").unwrap();
                }

                if ui.button("Quit").clicked() {
                    commands.events.submit(Stop);
                }
            });
        });
    }
}
