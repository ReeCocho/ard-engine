use rfd::FileDialog;
use std::path::{Path, PathBuf};

use ard_engine::{game::SceneGameObject, log::warn};

use crate::util::editor_job::EditorJob;

use super::View;

#[derive(Default)]
pub struct ToolBar {}

impl View for ToolBar {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        _controller: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
    ) {
        ui.main_menu_bar(|| {
            ui.menu("File", || {
                if ui.menu_item("Save") {
                    self.save(resc);
                }
            });

            ui.menu("New", || {
                if ui.menu_item("Static Object") {
                    resc.scene_graph
                        .create(SceneGameObject::StaticObject, &resc.ecs_commands.entities);
                }
            });
        });
    }
}

impl ToolBar {
    fn save(&self, resc: &mut crate::editor::Resources) {
        // Flush dirty assets
        resc.jobs.add(resc.dirty.flush(resc.assets));

        // Save the scene
        let descriptor = resc.scene_graph.save(resc.queries, resc.assets);

        match resc.scene_graph.asset() {
            // If this scene was loaded from somewhere, save to that file
            Some(handle) => {
                // Compute the path to the asset
                let mut path = PathBuf::from("./assets/game/");
                path.push(resc.assets.get_name(handle));

                // Add the job to save the file
                resc.jobs.add(EditorJob::new(
                    "Save Scene",
                    None,
                    move || {
                        let as_str = ron::ser::to_string_pretty(
                            &descriptor,
                            ron::ser::PrettyConfig::default(),
                        )
                        .unwrap();
                        std::fs::write(path, as_str).unwrap();
                    },
                    move |ui| {
                        let style = unsafe { ui.style() };
                        ui.text("Saving scene...");
                        ui.same_line();
                        crate::util::ui::throbber(
                            ui,
                            8.0,
                            4.0,
                            8,
                            1.0,
                            style[imgui::StyleColor::Button],
                        );
                    },
                ));
            }
            // If this scene was not loaded, create a save dialouge
            None => {
                let assets_cl = resc.assets.clone();
                let notify = resc.scene_graph.load_scene_channel();

                resc.jobs.add(EditorJob::new(
                    "Save Scene",
                    None,
                    move || {
                        // Dialouge to save the scene
                        let file = FileDialog::new()
                            .add_filter("scene file", &["scene"])
                            .set_directory(Path::new("./assets/game/"))
                            .save_file();

                        let path = match file {
                            Some(path) => path,
                            None => return,
                        };

                        // Write to the file
                        let as_str = ron::ser::to_string_pretty(
                            &descriptor,
                            ron::ser::PrettyConfig::default(),
                        )
                        .unwrap();
                        std::fs::write(&path, as_str).unwrap();

                        // Determine the possible asset name
                        let path = path.canonicalize().unwrap();
                        let root_path = PathBuf::from("./assets/game/").canonicalize().unwrap();

                        let mut asset_name = PathBuf::new();
                        for component in path.iter().skip(root_path.iter().count()) {
                            asset_name.push(component);
                        }

                        // Scan for the asset
                        if assets_cl.scan_for(&asset_name) {
                            // Get the asset handle
                            let handle = assets_cl.load(&asset_name);

                            // Notify the scene graph that the scene has been saved
                            let _ = notify.send((handle, false));
                        } else {
                            warn!("failure to detect saved scene at location `{:?}`", &path);
                        }
                    },
                    move |ui| {
                        let style = unsafe { ui.style() };
                        ui.text("Saving scene...");
                        ui.same_line();
                        crate::util::ui::throbber(
                            ui,
                            8.0,
                            4.0,
                            8,
                            1.0,
                            style[imgui::StyleColor::Button],
                        );
                    },
                ));
            }
        }
    }
}
