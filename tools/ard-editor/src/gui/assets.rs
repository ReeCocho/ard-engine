use std::{ffi::OsStr, path::PathBuf};

use crate::{
    assets::{meta::MetaFile, EditorAssets},
    tasks::{
        asset::{NewFolderTask, RenameAssetTask, RenameFolderTask},
        TaskQueue,
    },
};

use super::{drag_drop::DragDropPayload, EditorViewContext};

#[derive(Default)]
pub struct AssetsView {
    cur_path: PathBuf,
}

impl AssetsView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut assets = ctx.res.get_mut::<EditorAssets>().unwrap();
        let tasks = ctx.res.get::<TaskQueue>().unwrap();

        let folder = match assets.find_folder_mut(&self.cur_path) {
            Some(folder) => folder,
            None => {
                self.cur_path.pop();
                return egui_tiles::UiResponse::None;
            }
        };

        let mut new_folder = false;

        let rect = ctx.ui.available_rect_before_wrap();
        ctx.ui
            .interact(
                rect,
                egui::Id::new("asset_view_interact"),
                egui::Sense::click(),
            )
            .context_menu(|ui| {
                if ui.button("Back").clicked() {
                    self.cur_path.pop();
                    new_folder = true;
                }

                if ui.button("New Folder").clicked() {
                    tasks.add(NewFolderTask::new(&self.cur_path));
                }
            });
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .hscroll(false)
            .show(ctx.ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (folder_name, folder) in folder.sub_folders() {
                        let response = Self::folder_ui(ui, folder_name);
                        if response.double_clicked() {
                            self.cur_path.push(folder_name);
                            new_folder = true;
                        }

                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                tasks.add(RenameFolderTask::new(folder.abs_path()));
                            }
                        });
                    }

                    for (_, asset) in folder.assets() {
                        let response = Self::asset_ui(
                            ui,
                            asset
                                .raw_path
                                .file_name()
                                .unwrap_or(OsStr::new("(INVALID CHARACTERS)")),
                            &asset.meta,
                        );
                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                tasks.add(RenameAssetTask::new(asset.clone()));
                            }
                        });
                    }
                });
            });

        if new_folder {
            assets
                .find_folder_mut(&self.cur_path)
                .map(|folder| folder.inspect());
        }

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

    fn asset_ui(ui: &mut egui::Ui, file_name: &OsStr, meta_file: &MetaFile) -> egui::Response {
        let icon = egui::RichText::new(egui_phosphor::fill::FILE).size(64.0);
        let size = [100.0, 160.0];
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
        let asset_name = file_name.to_string_lossy();

        ui.allocate_ui_with_layout(size.into(), layout, |ui| {
            ui.group(|ui| {
                ui.vertical_centered_justified(|ui| {
                    let r = ui
                        .dnd_drag_source(
                            egui::Id::new(&meta_file.baked),
                            DragDropPayload::Asset(meta_file.clone()),
                            |ui| {
                                ui.add(
                                    egui::Label::new(icon)
                                        .selectable(false)
                                        .sense(egui::Sense::click()),
                                );
                            },
                        )
                        .response;
                    ui.add(
                        egui::Label::new(asset_name)
                            .selectable(false)
                            .sense(egui::Sense::click())
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
}
