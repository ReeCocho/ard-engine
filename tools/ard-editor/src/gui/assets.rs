use std::path::PathBuf;

use ard_engine::{
    assets::package::PackageId,
    ecs::{resource::res::Res, system::data::Everything},
};
use camino::Utf8PathBuf;

use crate::{
    assets::{op::AssetOpInstance, EditorAsset, EditorAssets, Folder},
    tasks::{asset::RenameAssetOp, TaskQueue},
};

use super::{drag_drop::DragDropPayload, util, EditorViewContext};

#[derive(Default)]
pub struct AssetsView {
    cur_path: Utf8PathBuf,
}

impl AssetsView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let assets = ctx.res.get_mut::<EditorAssets>().unwrap();
        let active_package = assets.active_package_id();

        let folder = match assets.find_folder(&self.cur_path) {
            Some(folder) => folder,
            None => {
                self.cur_path.pop();
                return egui_tiles::UiResponse::None;
            }
        };

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
                }

                if ui.button("New Folder").clicked() {
                    /*
                    ctx.res
                        .get_mut::<TaskQueue>()
                        .unwrap()
                        .add(NewFolderTask::new(&self.cur_path, &package_root));
                    */
                }
            });

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .hscroll(false)
            .show(ctx.ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (folder_name, folder) in folder.sub_folders() {
                        let response = Self::folder_ui(ui, ctx.res, folder, folder_name);
                        if response.double_clicked() {
                            self.cur_path.push(folder_name);
                        }

                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                /*
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(RenameFolderTask::new(folder.abs_path()));
                                */
                            }
                        });
                    }

                    for (_, asset) in folder.assets() {
                        let response = Self::asset_ui(
                            ui,
                            asset
                                .meta_path()
                                .with_extension("")
                                .file_name()
                                .unwrap_or("(INVALID CHARACTERS)"),
                            &asset,
                            active_package,
                        );
                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(AssetOpInstance::new(RenameAssetOp::new(&assets, asset)));
                            }

                            if self.cur_path != PathBuf::default()
                                && ui.button("Move up a folder").clicked()
                            {
                                /*
                                let task_queue = ctx.res.get_mut::<TaskQueue>().unwrap();
                                let mut dst = self.cur_path.clone();
                                dst.pop();
                                let dst = path!(folder.package_root() / dst);

                                task_queue.add(MoveTask::new(&asset.raw_path, &dst));
                                task_queue.add(MoveTask::new(&asset.meta_path, &dst));
                                */
                            }
                        });
                    }
                });
            });

        egui_tiles::UiResponse::None
    }

    fn folder_ui(
        ui: &mut egui::Ui,
        _res: &Res<Everything>,
        _folder: &Folder,
        name: &str,
    ) -> egui::Response {
        let icon = egui::RichText::new(egui_phosphor::fill::FOLDER).size(64.0);
        let size = [100.0, 160.0];
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());

        let result = util::hidden_drop_zone::<DragDropPayload, _>(ui, egui::Frame::none(), |ui| {
            ui.allocate_ui_with_layout(size.into(), layout, |ui| {
                ui.group(|ui| {
                    ui.vertical_centered_justified(|ui| {
                        let r = ui.add(
                            egui::Label::new(icon)
                                .selectable(false)
                                .sense(egui::Sense::click()),
                        );
                        ui.add(egui::Label::new(name).selectable(false).truncate(true));
                        r
                    })
                    .inner
                })
                .inner
            })
            .inner
        });

        if let Some(dropped) = result.1 {
            match dropped.as_ref() {
                DragDropPayload::Asset(_asset) => {
                    /*
                    let task_queue = res.get_mut::<TaskQueue>().unwrap();
                    task_queue.add(MoveTask::new(&asset.raw_path, folder.abs_path()));
                    task_queue.add(MoveTask::new(&asset.meta_path, folder.abs_path()));
                    */
                }
                _ => {}
            }
        }

        result.0.inner
    }

    fn asset_ui(
        ui: &mut egui::Ui,
        file_name: &str,
        asset: &EditorAsset,
        active_package: PackageId,
    ) -> egui::Response {
        let icon = egui::RichText::new(egui_phosphor::fill::FILE).size(64.0);
        let size = [100.0, 160.0];
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());

        let label = match (asset.is_shadowing(), active_package == asset.package()) {
            (true, true) => format!("{} {file_name}", egui_phosphor::fill::LOCK_SIMPLE_OPEN),
            (_, false) => format!("{} {file_name}", egui_phosphor::fill::LOCK_SIMPLE),
            _ => file_name.to_owned(),
        };

        ui.allocate_ui_with_layout(size.into(), layout, |ui| {
            ui.group(|ui| {
                ui.vertical_centered_justified(|ui| {
                    let r = ui
                        .dnd_drag_source(
                            egui::Id::new(&asset.meta_file().baked),
                            DragDropPayload::Asset(asset.clone()),
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
                        egui::Label::new(label)
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
