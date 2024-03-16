use ard_core::core::Tick;
use ard_ecs::prelude::*;

pub trait GuiView {
    fn show(
        &mut self,
        tick: Tick,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    );
}
