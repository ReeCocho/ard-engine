use egui::Widget;

use crate::tasks::{TaskQueue, TaskState};

use super::EditorViewContext;

#[derive(Default)]
pub struct TaskQueueView {
    states: Vec<TaskState>,
}

impl TaskQueueView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let queue = ctx.res.get::<TaskQueue>().unwrap();
        while let Some(state) = queue.recv_state() {
            self.states.push(state);
        }

        if ctx.ui.button("Clear").clicked() {
            self.states.clear();
        }
        ctx.ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ctx.ui, |ui| {
                self.states.iter().for_each(|state| {
                    ui.label(state.name());
                    ui.horizontal(|ui| {
                        let success = egui::Label::new(
                            egui::RichText::new(egui_phosphor::regular::CHECK).size(32.0),
                        )
                        .selectable(false);
                        let fail = egui::Label::new(
                            egui::RichText::new(egui_phosphor::regular::X).size(32.0),
                        )
                        .selectable(false);

                        match state.succeeded() {
                            Some(true) => {
                                ui.add(success);
                            }
                            Some(false) => {
                                ui.add(fail);
                            }
                            None => {
                                egui::Spinner::new().size(32.0).ui(ui);
                            }
                        }
                        ui.add(egui::ProgressBar::new(state.completion()).show_percentage());
                    });
                    ui.separator();
                });
            });

        egui_tiles::UiResponse::None
    }
}
