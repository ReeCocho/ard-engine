use crate::Backend;
use ard_ecs::prelude::*;
use imgui::Ui;

pub trait DebugGuiApi<B: Backend>: Resource + Send + Sync {
    fn ui(&mut self) -> &Ui;

    fn begin_dock(&mut self);

    fn font_atlas() -> imgui::TextureId;

    fn scene_view() -> imgui::TextureId;
}
