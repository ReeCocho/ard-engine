use ard_ecs::prelude::*;

pub trait GuiView {
    fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    );
}
