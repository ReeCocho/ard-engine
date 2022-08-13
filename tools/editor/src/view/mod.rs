pub mod assets;
pub mod hierarchy;
pub mod inspector;
pub mod lighting;
pub mod scene_view;
pub mod toolbar;

use crate::{controller::Controller, editor::Resources};

pub trait View {
    fn show(&mut self, ui: &imgui::Ui, controller: &mut Controller, resc: &mut Resources);
}
