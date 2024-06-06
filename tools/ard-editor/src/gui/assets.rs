use std::ffi::OsStr;

use crate::assets::{meta::MetaFile, EditorAssets};

use super::EditorViewContext;

pub struct AssetsView {}

impl AssetsView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let assets = ctx.res.get::<EditorAssets>().unwrap();
        ctx.ui.horizontal_wrapped(|ui| {
            for (folder_name, _) in assets.root().sub_folders() {
                let response = Self::folder_ui(ui, folder_name);
                if response.double_clicked() {
                    println!("go into folder...");
                }
            }

            for (_, meta_file) in assets.root().assets() {
                Self::asset_ui(ui, meta_file);
            }
        });

        egui_tiles::UiResponse::None
    }

    fn folder_ui(ui: &mut egui::Ui, name: &OsStr) -> egui::Response {
        let icon = egui::RichText::new(egui_phosphor::fill::FOLDER).size(64.0);
        let size = [100.0, 160.0];
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
        let folder_name = name.to_string_lossy();

        ui.allocate_ui_with_layout(size.into(), layout, |ui| {
            ui.group(|ui| {
                ui.vertical_centered_justified(|ui| {
                    let r = ui.add(
                        egui::Label::new(icon)
                            .selectable(false)
                            .sense(egui::Sense::click()),
                    );
                    ui.add(
                        egui::Label::new(folder_name)
                            .selectable(false)
                            .truncate(true),
                    );
                    r
                })
                .inner
            })
            .inner
        })
        .inner
    }

    fn asset_ui(ui: &mut egui::Ui, meta_file: &MetaFile) {
        let icon = egui::RichText::new(egui_phosphor::fill::FILE).size(64.0);
        let size = [100.0, 160.0];
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
        let asset_name = meta_file.raw.file_name().unwrap().to_string_lossy();

        ui.allocate_ui_with_layout(size.into(), layout, |ui| {
            ui.group(|ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.dnd_drag_source(
                        egui::Id::new(&meta_file.baked),
                        String::from("blarg"),
                        |ui| {
                            ui.add(
                                egui::Label::new(icon)
                                    .selectable(false)
                                    .sense(egui::Sense::click()),
                            );
                        },
                    );
                    ui.add(
                        egui::Label::new(asset_name)
                            .selectable(false)
                            .truncate(true),
                    )
                })
            })
        });
    }
}
