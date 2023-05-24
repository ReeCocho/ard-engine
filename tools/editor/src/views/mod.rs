pub mod scene;

use ard_engine::ecs::prelude::Resource;
use egui_dock::Tree;
use scene::SceneView;

use crate::editor::EditorPresenter;

editor_panels::define!(SceneView);

#[derive(Resource)]
pub struct EditorDockTree(pub Tree<EditorPanelType>);

impl Default for EditorDockTree {
    fn default() -> Self {
        let mut tree = Tree::new(vec![EditorPanelType::SceneView]);
        // let [top, _bottom] = tree.split_below(
        //     NodeIndex::root(),
        //     0.8,
        //     vec![EditorViewType::Assets.to_owned()],
        // );
        // let [old, _new] =
        //     tree.split_left(top, 0.15, vec![EditorViewType::EntityHierarchy.to_owned()]);
        // let _ = tree.split_right(old, 0.75, vec![EditorViewType::Inspector.to_owned()]);

        EditorDockTree(tree)
    }
}

impl EditorDockTree {
    pub fn show(&mut self, ctx: &egui::Context, mut presenter: EditorPresenter) {
        egui_dock::DockArea::new(&mut self.0)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut presenter);
    }
}

mod editor_panels {
    macro_rules! define {
        ( $($panel:ty)* ) => {
            paste::paste!{
                #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
                pub enum EditorPanelType {
                    $(
                        [<$panel>],
                    )*
                }

                #[allow(non_snake_case)]
                #[derive(Resource)]
                pub struct EditorPanels {
                    $(
                        pub [<panel_ $panel>]: $panel,
                    )*
                }
            }
        };
    }

    pub(crate) use define;
}
