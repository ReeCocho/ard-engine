use ard_engine::{core::core::Stop, ecs::prelude::*};

pub struct MenuBar;

impl MenuBar {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    commands.events.submit(Stop);
                }
            });
        });
    }
}
