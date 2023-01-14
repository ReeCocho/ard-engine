use crate::{
    drag_drog::{DragDrop, DragDropData},
    models::{
        scene::{SceneViewMessage, SceneViewModel},
        ViewModelInstance,
    },
};

use super::View;
use ard_engine::{
    math::*,
    render::{prelude::ModelAsset, renderer::Renderer},
};

pub struct SceneView;

impl SceneView {
    pub fn new() -> Self {
        Self {}
    }
}

impl View for SceneView {
    type ViewModel = SceneViewModel;

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        drag_drop: &mut DragDrop,
        view_model: &mut ViewModelInstance<Self::ViewModel>,
    ) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Create", |ui| if ui.button("Cube").clicked() {});
        });

        let available_size = ui.available_size();
        view_model.vm.view_size = (
            (available_size.x as u32).max(1),
            (available_size.y as u32).max(1),
        );

        let response = ui.add(
            egui::widgets::Image::new(Renderer::egui_texture_id(), available_size)
                .sense(egui::Sense::drag()),
        );
        view_model.vm.looking = response.dragged();

        if response.hovered() {
            if let Some(data) = drag_drop.recv() {
                match data {
                    DragDropData::Asset(name) => {
                        // std::thread::sleep(std::time::Duration::from_secs(3));
                        view_model
                            .messages
                            .send(SceneViewMessage::InstantiateModel {
                                model: view_model.vm.assets.load::<ModelAsset>(&name),
                                root: None,
                            })
                    }
                }
            }
        }
    }

    fn title(&self) -> egui::WidgetText {
        "Scene".into()
    }
}
