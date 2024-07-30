pub mod assets;
pub mod drag_drop;
pub mod hierarchy;
pub mod inspector;
pub mod menu_bar;
pub mod scene;
pub mod task_queue;
pub mod transform;
pub mod util;

use ard_engine::{core::prelude::*, ecs::prelude::*, render::view::GuiView};
use hierarchy::HierarchyView;
use inspector::InspectorView;
use task_queue::TaskQueueView;

use self::{assets::AssetsView, menu_bar::MenuBar, scene::SceneView};

#[derive(Debug, Clone, Copy)]
pub enum Pane {
    Scene,
    Assets,
    Hierarchy,
    Inspector,
    TaskQueue,
}

pub struct EditorView {
    tree: egui_tiles::Tree<Pane>,
    menu_bar: MenuBar,
    scene: SceneView,
    assets: AssetsView,
    hierarchy: HierarchyView,
    inspector: InspectorView,
    task_queue: TaskQueueView,
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
    inspector: &'a mut InspectorView,
    task_queue: &'a mut TaskQueueView,
}

impl Default for EditorView {
    fn default() -> Self {
        let mut tiles = egui_tiles::Tiles::default();

        let assets = tiles.insert_pane(Pane::Assets);
        let task_queue = tiles.insert_pane(Pane::TaskQueue);

        let vertical = vec![
            tiles.insert_pane(Pane::Scene),
            tiles.insert_container(egui_tiles::Tabs::new(vec![assets, task_queue])),
        ];

        let horizontal = vec![
            tiles.insert_pane(Pane::Hierarchy),
            tiles.insert_vertical_tile(vertical),
            tiles.insert_pane(Pane::Inspector),
        ];

        let root = tiles.insert_horizontal_tile(horizontal);

        EditorView {
            tree: egui_tiles::Tree::new("editor_view_tree", root, tiles),
            menu_bar: MenuBar,
            scene: SceneView::default(),
            assets: AssetsView::default(),
            hierarchy: HierarchyView::default(),
            inspector: InspectorView::default(),
            task_queue: TaskQueueView::default(),
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
        egui::Frame::window(ui.style())
            .inner_margin(pane.margin())
            .outer_margin(egui::Margin::ZERO)
            .stroke(egui::Stroke::NONE)
            .show(ui, |ui| {
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
                    Pane::Inspector => self.inspector.show(ctx),
                    Pane::TaskQueue => self.task_queue.show(ctx),
                }
            })
            .inner
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    fn tab_bar_color(&self, visuals: &egui::Visuals) -> egui::Color32 {
        (egui::Rgba::from(visuals.panel_fill) * egui::Rgba::from_gray(0.6)).into()
    }

    fn tab_outline_stroke(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &egui_tiles::Tiles<Pane>,
        _tile_id: egui_tiles::TileId,
        _active: &egui_tiles::TabState,
    ) -> egui::Stroke {
        egui::Stroke::NONE
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        2.0
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
                inspector: &mut self.inspector,
                task_queue: &mut self.task_queue,
            };
            self.tree.ui(&mut behavior, ui);
        });
    }
}

impl Pane {
    fn margin(&self) -> egui::Margin {
        match self {
            Pane::Scene => egui::Margin::ZERO,
            _ => egui::Margin {
                left: 4.0,
                right: 4.0,
                top: 4.0,
                bottom: 4.0,
            },
        }
    }
}
