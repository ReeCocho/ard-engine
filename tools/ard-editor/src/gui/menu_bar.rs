use ard_engine::{
    assets::manager::Assets,
    core::core::Stop,
    ecs::prelude::*,
    game::components::{
        destroy::Destroy,
        transform::{Children, Parent, Position, Rotation, Scale, SetParent},
    },
    render::{
        loader::{MaterialHandle, MeshHandle},
        prelude::RenderingMode,
        MaterialInstance, Mesh, Model, RenderFlags,
    },
    save_load::{
        format::Ron,
        load_data::Loader,
        save_data::{SaveData, Saver},
    },
};

use crate::{inspect::transform::EulerRotation, scene_graph::SceneGraph};

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
                    let (save_data, _) =
                        crate::ser::saver::<Ron>().save(assets.clone(), queries, &entities);

                    let save_data = bincode::serialize(&save_data).unwrap();
                    std::fs::write("./save.dat", &save_data).unwrap();
                }

                if ui.button("Load").clicked() {
                    let save_data = std::fs::read("./save.dat").unwrap();
                    let save_data = bincode::deserialize::<SaveData>(&save_data).unwrap();
                    crate::ser::loader::<Ron>().load(save_data, assets.clone(), &commands.entities);
                }

                if ui.button("Make Lof").clicked() {
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
