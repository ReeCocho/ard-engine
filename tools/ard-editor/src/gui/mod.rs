pub mod scene;

use ard_engine::{core::prelude::*, ecs::prelude::*, render::view::GuiView};

use self::scene::SceneView;

#[derive(Debug, Clone, Copy)]
pub enum Pane {
    Scene,
    Assets,
}

pub struct EditorView {
    tree: egui_tiles::Tree<Pane>,
    scene: SceneView,
}

pub struct EditorViewContext<'a> {
    pub tick: Tick,
    pub ui: &'a mut egui::Ui,
    pub commands: &'a Commands,
    pub queries: &'a Queries<Everything>,
    pub res: &'a Res<Everything>,
}

struct EditorViewBehavior<'a> {
    tick: Tick,
    commands: &'a Commands,
    queries: &'a Queries<Everything>,
    res: &'a Res<Everything>,
    scene: &'a mut SceneView,
}

impl Default for EditorView {
    fn default() -> Self {
        let mut tiles = egui_tiles::Tiles::default();

        let mut tabs = Vec::default();
        tabs.push(tiles.insert_pane(Pane::Scene));
        tabs.push(tiles.insert_pane(Pane::Assets));

        let root = tiles.insert_tab_tile(tabs);

        EditorView {
            tree: egui_tiles::Tree::new("editor_view_tree", root, tiles),
            scene: SceneView {},
        }
    }
}

impl<'a> egui_tiles::Behavior<Pane> for EditorViewBehavior<'a> {
    fn tab_title_for_pane(&mut self, _pane: &Pane) -> egui::WidgetText {
        "blarg".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let ctx = EditorViewContext {
            tick: self.tick,
            ui,
            commands: self.commands,
            queries: self.queries,
            res: self.res,
        };
        match *pane {
            Pane::Scene => self.scene.show(ctx),
            Pane::Assets => egui_tiles::UiResponse::None,
        }
    }
}

impl GuiView for EditorView {
    fn show(
        &mut self,
        tick: Tick,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut behavior = EditorViewBehavior {
                tick,
                commands,
                queries,
                res,
                scene: &mut self.scene,
            };
            self.tree.ui(&mut behavior, ui);
        });
    }
}
