use crate::Backend;
use ard_ecs::prelude::*;
use imgui::Ui;

pub trait DebugGuiApi<B: Backend>: Resource + Send + Sync {
    fn ui(&mut self) -> &mut Ui;
}
