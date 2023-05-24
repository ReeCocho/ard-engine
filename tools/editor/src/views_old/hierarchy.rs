use crate::{
    drag_drog::DragDrop,
    models::{
        scene::{SceneGraphNode, SceneViewModel},
        ViewModelInstance,
    },
};

use super::View;

#[derive(Default)]
pub struct HierarchyView;

impl View for HierarchyView {
    type ViewModel = SceneViewModel;

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        _drag_drop: &mut DragDrop,
        view_model: &mut ViewModelInstance<Self::ViewModel>,
    ) {
        fn draw_node(ui: &mut egui::Ui, node: &SceneGraphNode, i: &mut u32) {
            *i += 1;
            if ui
                .collapsing(i.to_string(), |ui| {
                    for node in &node.children {
                        draw_node(ui, node, i);
                    }
                })
                .header_response
                .dragged()
            {
                println!("T");
            }
        }

        let mut i = 0;
        for node in &view_model.vm.roots {
            draw_node(ui, node, &mut i);
        }
    }

    fn title(&self) -> egui::WidgetText {
        "Hierarchy".into()
    }
}
