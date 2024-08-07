use ard_engine::{
    assets::package::PackageId,
    ecs::{resource::res::Res, system::data::Everything},
};
use camino::Utf8PathBuf;

use crate::{
    assets::{meta::AssetType, CurrentAssetPath, EditorAsset, EditorAssets, Folder},
    selected::Selected,
    tasks::{
        asset::{
            DeleteAssetTask, DeleteFolderTask, MoveAssetTask, NewFolderTask, RenameAssetTask,
            RenameFolderTask,
        },
        material::CreateMaterialTask,
        TaskQueue,
    },
};

use super::{drag_drop::DragDropPayload, util, EditorViewContext};

#[derive(Default)]
pub struct AssetsView;

impl AssetsView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let assets = ctx.res.get_mut::<EditorAssets>().unwrap();
        let mut cur_path = ctx.res.get_mut::<CurrentAssetPath>().unwrap();

        let active_package = assets.active_package_id();

        let folder = match assets.find_folder(cur_path.path()) {
            Some(folder) => folder,
            None => {
                cur_path.path_mut().pop();
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
                    cur_path.path_mut().pop();
                }

                if ui.button("New Folder").clicked() {
                    ctx.res
                        .get_mut::<TaskQueue>()
                        .unwrap()
                        .add(NewFolderTask::new(cur_path.path()));
                }

                ui.menu_button("Create...", |ui| {
                    if ui.button("Material").clicked() {
                        ctx.res
                            .get_mut::<TaskQueue>()
                            .unwrap()
                            .add(CreateMaterialTask::default());
                    }
                });
            });

        ctx.ui.horizontal(|ui| {
            let mut new_path = None;

            let (_, payload) = ui.dnd_drop_zone::<DragDropPayload, _>(egui::Frame::none(), |ui| {
                if ui
                    .link(assets.active_assets_root().file_name().unwrap())
                    .clicked()
                {
                    new_path = Some(Utf8PathBuf::default());
                }
            });

            let mut payload = payload.map(|p| (p, Utf8PathBuf::default()));

            let mut cur = Utf8PathBuf::default();
            for component in cur_path.path().components() {
                cur.push(component);
                ui.label("/");
                let (_, new_payload) =
                    ui.dnd_drop_zone::<DragDropPayload, _>(egui::Frame::none(), |ui| {
                        if ui.link(component.as_str()).clicked() {
                            new_path = Some(cur.clone());
                        }
                    });

                if new_payload.is_some() {
                    payload = new_payload.map(|p| (p, cur.clone()));
                }
            }

            if let Some((payload, dst)) = payload {
                if let DragDropPayload::Asset(asset) = payload.as_ref() {
                    ctx.res
                        .get_mut::<TaskQueue>()
                        .unwrap()
                        .add(MoveAssetTask::new(asset, dst));
                }
            }

            if let Some(path) = new_path {
                *cur_path.path_mut() = path;
            }
        });

        ctx.ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .hscroll(false)
            .show(ctx.ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (folder_name, folder) in folder.sub_folders() {
                        let response = Self::folder_ui(ui, ctx.res, folder, folder_name);
                        if response.double_clicked() {
                            cur_path.path_mut().push(folder_name);
                        }

                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(RenameFolderTask::new(folder.path()));
                            }

                            if ui.button("Delete").clicked() {
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(DeleteFolderTask::new(folder.path()));
                            }
                        });
                    }

                    for (_, asset) in folder.assets() {
                        let (icon_r, text_r) = Self::asset_ui(
                            ui,
                            asset
                                .meta_path()
                                .with_extension("")
                                .file_name()
                                .unwrap_or("(INVALID CHARACTERS)"),
                            &asset,
                            active_package,
                        );

                        if text_r.clicked() {
                            *ctx.res.get_mut::<Selected>().unwrap() =
                                Selected::Asset(asset.meta_path().into());
                        }

                        icon_r.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(RenameAssetTask::new(asset));
                            }

                            if ui.button("Delete").clicked() {
                                ctx.res
                                    .get_mut::<TaskQueue>()
                                    .unwrap()
                                    .add(DeleteAssetTask::new(asset));
                            }
                        });
                    }
                });
            });

        egui_tiles::UiResponse::None
    }

    fn folder_ui(
        ui: &mut egui::Ui,
        res: &Res<Everything>,
        folder: &Folder,
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
                        ui.add(egui::Label::new(name).selectable(false).truncate());
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
                DragDropPayload::Asset(asset) => {
                    res.get_mut::<TaskQueue>()
                        .unwrap()
                        .add(MoveAssetTask::new(asset, folder.path()));
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
    ) -> (egui::Response, egui::Response) {
        let icon = match asset.meta_file().data.ty() {
            AssetType::Model => egui_phosphor::fill::CUBE,
            AssetType::Texture => egui_phosphor::fill::FILE_IMAGE,
            AssetType::Scene => egui_phosphor::fill::GLOBE,
            AssetType::Material => egui_phosphor::fill::SPHERE,
            AssetType::Mesh => egui_phosphor::regular::CUBE_TRANSPARENT,
        };

        let icon = egui::RichText::new(icon).size(64.0);
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
                    let icon_r = ui
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
                    let text_r = ui.add(
                        egui::Label::new(label)
                            .selectable(false)
                            .sense(egui::Sense::click())
                            .truncate(),
                    );
                    (icon_r, text_r)
                })
                .inner
            })
            .inner
        })
        .inner
    }
}
