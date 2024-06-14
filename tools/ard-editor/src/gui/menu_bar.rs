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
        load_data::Loader,
        save_data::{SaveData, Saver},
    },
};

use crate::scene_graph::SceneGraph;

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
                    let save_data = Saver::default()
                        .include_component::<Position>()
                        .include_component::<Rotation>()
                        .include_component::<Scale>()
                        .include_component::<Parent>()
                        .include_component::<Children>()
                        .include_component::<Model>()
                        .include_component::<RenderingMode>()
                        .include_component::<RenderFlags>()
                        .include_component::<MeshHandle>()
                        .include_component::<MaterialHandle>()
                        .ignore::<Mesh>()
                        .ignore::<MaterialInstance>()
                        .ignore::<Destroy>()
                        .ignore::<SetParent>()
                        .save(assets.clone(), queries, &entities);

                    let save_data = bincode::serialize(&save_data).unwrap();
                    std::fs::write("./save.dat", &save_data).unwrap();
                }

                if ui.button("Load").clicked() {
                    let save_data = std::fs::read("./save.dat").unwrap();
                    let save_data = bincode::deserialize::<SaveData>(&save_data).unwrap();
                    Loader::default()
                        .load_component::<Position>()
                        .load_component::<Rotation>()
                        .load_component::<Scale>()
                        .load_component::<Parent>()
                        .load_component::<Children>()
                        .load_component::<Model>()
                        .load_component::<RenderingMode>()
                        .load_component::<RenderFlags>()
                        .load_component::<MeshHandle>()
                        .load_component::<MaterialHandle>()
                        .load(save_data, assets.clone(), &commands.entities);
                }

                if ui.button("Quit").clicked() {
                    commands.events.submit(Stop);
                }
            });
        });
    }
}
