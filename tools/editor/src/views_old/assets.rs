use crate::{
    asset_import::AssetImportState,
    drag_drog::{DragDrop, DragDropData},
    models::assets::{AssetsViewMessage, AssetsViewModel},
    util::par_task::ParTaskGet,
};

use super::{View, ViewModelInstance};
use ard_engine::{assets::prelude::*, ecs::prelude::*, render::asset::texture::TextureAsset};
use egui::WidgetText;

const ASSET_ICON_SIZE: f32 = 80.0;

pub struct AssetImportView;

pub struct AssetsView {
    assets: Assets,
    icons: AssetsViewIcons,
}

struct AssetsViewIcons {
    file: Handle<TextureAsset>,
    folder: Handle<TextureAsset>,
}

impl AssetsView {
    pub fn new(assets: &Assets) -> Self {
        // Load icons
        let icons = AssetsViewIcons {
            file: assets.load(AssetName::new("_editor/icons/file.tex")),
            folder: assets.load(AssetName::new("_editor/icons/folder.tex")),
        };

        assets.wait_for_load(&icons.file);
        assets.wait_for_load(&icons.folder);

        AssetsView {
            assets: assets.clone(),
            icons,
        }
    }
}

impl ard_engine::render::renderer::gui::View for AssetImportView {
    fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut state = res.get_mut::<AssetImportState>().unwrap();
        if !state.loading.is_empty() {
            let can_close = match state.loading.front_mut().unwrap().get() {
                ParTaskGet::Running => false,
                _ => true,
            };

            if can_close {
                let mut open = true;

                egui::Window::new("Importing")
                    .collapsible(false)
                    .resizable(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        let mut loaded = false;
                        state.loading.front_mut().unwrap().ui(ui, |_| {
                            loaded = true;
                        });

                        if loaded {
                            state.loading.pop_front();
                        }
                    });

                if !open {
                    state.loading.pop_front();
                }
            } else {
                egui::Window::new("Importing")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        let mut loaded = false;
                        state.loading.front_mut().unwrap().ui(ui, |_| {
                            loaded = true;
                        });

                        if loaded {
                            state.loading.pop_front();
                        }
                    });
            }
        }
    }
}

impl View for AssetsView {
    type ViewModel = AssetsViewModel;

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        drag_drop: &mut DragDrop,
        instance: &mut ViewModelInstance<AssetsViewModel>,
    ) {
        let folder = match instance.vm.get_active_folder() {
            Some(folder) => folder,
            None => {
                instance.messages.send(AssetsViewMessage::SetActiveFolder {
                    old: Vec::default(),
                    new: Vec::default(),
                });
                return;
            }
        };

        egui::menu::bar(ui, |ui| {
            if ui.button("Root").clicked() {
                instance.messages.send(AssetsViewMessage::SetActiveFolder {
                    old: Vec::from(instance.vm.get_folder_path_components()),
                    new: Vec::default(),
                });
            }

            for folder in instance.vm.get_folder_path_components() {
                ui.label(folder);
                ui.label(">");
            }
        });

        ui.separator();

        let folder_tex = self
            .assets
            .get(&self.icons.folder)
            .unwrap()
            .texture
            .egui_texture_id();
        let file_tex = self
            .assets
            .get(&self.icons.file)
            .unwrap()
            .texture
            .egui_texture_id();

        ui.horizontal_wrapped(|ui| {
            // Draw folders
            for sub_folder in &folder.folders {
                if draw_asset_icon(ui, folder_tex, &sub_folder.name).double_clicked() {
                    instance
                        .messages
                        .send(AssetsViewMessage::PushFolder(sub_folder.name.clone()));
                }
            }

            // Draw assets
            for asset in &folder.assets {
                if draw_asset_icon(ui, file_tex, &asset.display_name).drag_started() {
                    drag_drop.set_data(DragDropData::Asset(asset.asset_name.clone()));
                }
            }
        });
    }

    #[inline(always)]
    fn title(&self) -> egui::WidgetText {
        "Assets".into()
    }
}

fn draw_asset_icon(
    ui: &mut egui::Ui,
    tex: egui::TextureId,
    label: impl Into<WidgetText>,
) -> egui::Response {
    let text_widget: WidgetText = label.into();
    let valign = ui.layout().vertical_align();
    let mut text_job = text_widget.into_text_job(ui.style(), egui::FontSelection::Default, valign);
    text_job.job.wrap.max_width = ASSET_ICON_SIZE;
    text_job.job.wrap.max_rows = 1;
    text_job.job.halign = ui.layout().horizontal_placement();
    text_job.job.justify = ui.layout().horizontal_justify();

    let text_galley = text_job.into_galley(&ui.fonts());

    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(ASSET_ICON_SIZE, ASSET_ICON_SIZE)
            + egui::Vec2::new(0.0, text_galley.size().y),
        egui::Sense::click_and_drag(),
    );

    if ui.is_rect_visible(rect) {
        use egui::epaint::*;

        if response.hovered() {
            let mut mesh = Mesh::default();
            mesh.add_colored_rect(
                rect,
                if response.is_pointer_button_down_on() {
                    ui.style().visuals.text_color()
                } else {
                    ui.style().visuals.weak_text_color()
                },
            );
            ui.painter().add(Shape::mesh(mesh));
        }

        let mut mesh = Mesh::with_texture(tex);
        mesh.add_rect_with_uv(
            Rect::from_min_max(
                rect.min,
                egui::Pos2::new(rect.max.x, rect.max.y - text_galley.size().y),
            ),
            egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        ui.painter().add(Shape::mesh(mesh));

        ui.painter().add(egui::epaint::TextShape {
            pos: rect.center_bottom()
                - egui::Vec2::new(text_galley.size().x / 2.0, text_galley.size().y),
            galley: text_galley.galley,
            override_text_color: Some(ui.style().visuals.strong_text_color()),
            underline: egui::Stroke::NONE,
            angle: 0.0,
        });
    }

    response
}
