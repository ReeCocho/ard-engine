pub mod assets;
pub mod drag_drop;
pub mod hierarchy;
pub mod menu_bar;
pub mod scene;

use ard_engine::{core::prelude::*, ecs::prelude::*, render::view::GuiView};
use hierarchy::HierarchyView;

use self::{assets::AssetsView, menu_bar::MenuBar, scene::SceneView};

#[derive(Debug, Clone, Copy)]
pub enum Pane {
    Scene,
    Assets,
    Hierarchy,
}

pub struct EditorView {
    tree: egui_tiles::Tree<Pane>,
    menu_bar: MenuBar,
    scene: SceneView,
    assets: AssetsView,
    hierarchy: HierarchyView,
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
    assets: &'a mut AssetsView,
    hierarchy: &'a mut HierarchyView,
}

impl Default for EditorView {
    fn default() -> Self {
        let mut tiles = egui_tiles::Tiles::default();

        let mut tabs = Vec::default();
        tabs.push(tiles.insert_pane(Pane::Scene));
        tabs.push(tiles.insert_pane(Pane::Assets));
        tabs.push(tiles.insert_pane(Pane::Hierarchy));

        let root = tiles.insert_tab_tile(tabs);

        EditorView {
            tree: egui_tiles::Tree::new("editor_view_tree", root, tiles),
            menu_bar: MenuBar,
            scene: SceneView {},
            assets: AssetsView {},
            hierarchy: HierarchyView {},
        }
    }
}

impl<'a> egui_tiles::Behavior<Pane> for EditorViewBehavior<'a> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        format! {"{pane:?}"}.into()
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
            Pane::Assets => self.assets.show(ctx),
            Pane::Hierarchy => self.hierarchy.show(ctx),
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
            self.menu_bar.show(ui, commands, queries, res);

            let mut behavior = EditorViewBehavior {
                tick,
                commands,
                queries,
                res,
                scene: &mut self.scene,
                assets: &mut self.assets,
                hierarchy: &mut self.hierarchy,
            };
            self.tree.ui(&mut behavior, ui);
        });
    }
}
