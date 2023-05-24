use crate::{
    drag_drog::DragDrop,
    models::{ViewModel, ViewModelInstance},
};

pub mod assets;
pub mod hierarchy;
pub mod scene;

pub trait View {
    type ViewModel: ViewModel;

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        drag_drop: &mut DragDrop,
        view_model: &mut ViewModelInstance<Self::ViewModel>,
    );

    fn title(&self) -> egui::WidgetText;
}
