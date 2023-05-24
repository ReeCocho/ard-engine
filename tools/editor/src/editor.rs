use crate::views::{EditorDockTree, EditorPanelType, EditorPanels};
use ard_engine::{
    core::prelude::Stop,
    ecs::prelude::{Commands, Everything, Queries, Res},
    render::renderer::gui::View,
};

/// Takes the views and presents them using the `egui_dock` tab viewer.
pub struct EditorPresenter<'a> {
    pub panels: &'a mut EditorPanels,
    pub commands: &'a Commands,
    pub queries: &'a Queries<Everything>,
    pub res: &'a Res<Everything>,
}

// Main view of the system.
pub struct EditorGuiView;

/// Each dockable panel in the editor must implement this trait.
pub trait EditorPanel {
    fn title(&self) -> egui::WidgetText;

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    );
}

pub enum Command {}

impl View for EditorGuiView {
    fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut dock_space = res.get_mut::<EditorDockTree>().unwrap();
        let mut panels = res.get_mut::<EditorPanels>().unwrap();

        egui::TopBottomPanel::top("MenuBar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        commands.events.submit(Stop);
                    }
                });
            });
        });

        dock_space.show(
            ctx,
            EditorPresenter {
                panels: &mut panels,
                commands,
                queries,
                res,
            },
        );
    }
}

impl<'a> egui_dock::TabViewer for EditorPresenter<'a> {
    type Tab = EditorPanelType;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match *tab {
            EditorPanelType::SceneView => {
                self.panels
                    .panel_SceneView
                    .show(ui, self.commands, self.queries, self.res)
            }
        };
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            EditorPanelType::SceneView => self.panels.panel_SceneView.title(),
        }
    }
}
