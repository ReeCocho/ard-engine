use ard_engine::assets::prelude::Assets;

use crate::editor_job::EditorJobQueue;

use super::dirty_assets::DirtyAssets;

#[derive(Default)]
pub struct ToolBar {}

impl ToolBar {
    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        assets: &Assets,
        dirty: &mut DirtyAssets,
        jobs: &mut EditorJobQueue,
    ) {
        ui.main_menu_bar(|| {
            ui.menu("File", || {
                if ui.menu_item("Save") {
                    jobs.add(dirty.flush(assets));
                }
            });
        });
    }
}
