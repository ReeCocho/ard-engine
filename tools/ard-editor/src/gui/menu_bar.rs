use ard_engine::{
    assets::manager::Assets,
    core::core::Stop,
    ecs::prelude::*,
    save_load::{format::Ron, save_data::SaveData},
};

use crate::{assets::EditorAssets, scene_graph::SceneGraph};

pub struct MenuBar;

impl MenuBar {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                let assets = res.get::<Assets>().unwrap();
                if ui.button("Saved").clicked() {
                    let entities = res.get::<SceneGraph>().unwrap().all_entities(queries);
                    let res = crate::ser::saver::<Ron>().save(assets.clone(), queries, &entities);

                    match res {
                        Ok((save_data, _)) => {
                            let save_data = bincode::serialize(&save_data).unwrap();
                            std::fs::write("./save.dat", &save_data).unwrap();
                        }
                        Err(err) => {
                            println!("{err:?}");
                        }
                    }
                }

                if ui.button("Load").clicked() {
                    let save_data = std::fs::read("./save.dat").unwrap();
                    let save_data = bincode::deserialize::<SaveData>(&save_data).unwrap();
                    if let Err(err) = crate::ser::loader::<Ron>().load(
                        save_data,
                        assets.clone(),
                        &commands.entities,
                    ) {
                        println!("{err:?}");
                    }
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
                        "./test.lof",
                        "./packages/main/",
                    );
                }

                if ui.button("Quit").clicked() {
                    commands.events.submit(Stop);
                }
            });
        });
    }
}
