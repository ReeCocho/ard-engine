use crate::editor::EditorPanel;
use ard_engine::{ecs::prelude::{Commands, Everything, Queries, Res}, math::UVec2, render::renderer::Renderer};

pub struct SceneView {
    pub viewport_size: UVec2,
}

impl EditorPanel for SceneView {
    fn title(&self) -> egui::WidgetText {
        "Scene View".into()
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let available_size = ui.available_size();
        self.viewport_size = UVec2::new(
            (available_size.x as u32).max(1),
            (available_size.y as u32).max(1),
        );

        let response = ui.add(
            egui::widgets::Image::new(Renderer::egui_texture_id(), available_size)
                .sense(egui::Sense::drag()),
        );
    }
}

impl Default for SceneView {
    fn default() -> Self {
        Self {
            viewport_size: UVec2::new(100, 100),
        }
    }
}