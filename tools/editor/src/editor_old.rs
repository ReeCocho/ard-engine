use std::ops::{Deref, DerefMut};

use crate::{
    drag_drog::DragDrop,
    models::{
        assets::AssetsViewModel,
        scene::{SceneViewModel, SceneViewUpdate},
        ViewModelInstance,
    },
    views::{assets::AssetsView, hierarchy::HierarchyView, scene::SceneView, View},
};
use ard_engine::{
    assets::prelude::*,
    core::prelude::*,
    ecs::prelude::*,
    input::*,
    render::{prelude::Factory, renderer::RendererSettings},
};
use egui_dock::{DockArea, NodeIndex, Style, Tree};

pub struct EditorGuiView;

pub struct EditorPresenter<'a> {
    pub views: &'a mut EditorViews,
    pub vms: &'a mut EditorViewModels,
}

#[derive(Debug, Copy, Clone)]
pub enum EditorViewType {
    Assets,
    Inspector,
    Scene,
    EntityHierarchy,
}

#[derive(Resource)]
pub struct EditorDockTree(Tree<EditorViewType>);

#[derive(Resource)]
pub struct EditorViews {
    pub drag_drop: DragDrop,
    pub assets: AssetsView,
    pub scene: SceneView,
    pub hierarchy: HierarchyView,
}

#[derive(Resource)]
pub struct EditorViewModels {
    pub assets: ViewModelInstance<AssetsViewModel>,
    pub scene: ViewModelInstance<SceneViewModel>,
}

impl Default for EditorDockTree {
    fn default() -> Self {
        let mut tree = Tree::new(vec![EditorViewType::Scene]);
        let [top, _bottom] = tree.split_below(
            NodeIndex::root(),
            0.8,
            vec![EditorViewType::Assets.to_owned()],
        );
        let [old, _new] =
            tree.split_left(top, 0.15, vec![EditorViewType::EntityHierarchy.to_owned()]);
        let _ = tree.split_right(old, 0.75, vec![EditorViewType::Inspector.to_owned()]);

        EditorDockTree(tree)
    }
}

impl EditorViews {
    pub fn new(assets: &Assets) -> Self {
        Self {
            drag_drop: DragDrop::default(),
            assets: AssetsView::new(assets),
            scene: SceneView::new(),
            hierarchy: HierarchyView::default(),
        }
    }
}

impl EditorViewModels {
    pub fn new(assets: &Assets, factory: &Factory, commands: &EntityCommands) -> Self {
        Self {
            assets: ViewModelInstance::new(AssetsViewModel::new(assets)),
            scene: ViewModelInstance::new(SceneViewModel::new(assets, factory, commands)),
        }
    }

    pub fn undo(
        &mut self,
        ty: EditorViewType,
        entity_commands: &EntityCommands,
        res: &Res<Everything>,
    ) {
        let mut settings = res.get_mut::<RendererSettings>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let assets = res.get::<Assets>().unwrap();

        match ty {
            EditorViewType::Assets => self.assets.undo(&mut ()),
            EditorViewType::Inspector => todo!(),
            EditorViewType::Scene => self.scene.undo(&mut SceneViewUpdate {
                dt: 0.0,
                entity_commands,
                assets: assets.deref(),
                settings: settings.deref_mut(),
                input: input.deref(),
            }),
            EditorViewType::EntityHierarchy => todo!(),
        }
    }

    pub fn redo(
        &mut self,
        ty: EditorViewType,
        entity_commands: &EntityCommands,
        res: &Res<Everything>,
    ) {
        let mut settings = res.get_mut::<RendererSettings>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let assets = res.get::<Assets>().unwrap();

        match ty {
            EditorViewType::Assets => self.assets.redo(&mut ()),
            EditorViewType::Inspector => todo!(),
            EditorViewType::Scene => self.scene.redo(&mut SceneViewUpdate {
                dt: 0.0,
                entity_commands,
                assets: assets.deref(),
                settings: settings.deref_mut(),
                input: input.deref(),
            }),
            EditorViewType::EntityHierarchy => todo!(),
        }
    }
}

impl<'a> egui_dock::TabViewer for EditorPresenter<'a> {
    type Tab = EditorViewType;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match *tab {
            EditorViewType::Assets => {
                self.views
                    .assets
                    .show(ui, &mut self.views.drag_drop, &mut self.vms.assets)
            }
            EditorViewType::Inspector => {
                /*
                struct Blag {
                    x: u32,
                    y: f32,
                }

                impl crate::inspection::Inspectable for Blag {
                    fn inspect(&mut self, inspector: &mut impl crate::inspection::Inspector) {
                        inspector.inspect_u32("x", &mut self.x);
                        inspector.inspect_f32("y", &mut self.y);
                    }
                }

                let mut blag = Blag { x: 0, y: 0.0 };

                let mut inspector = crate::inspection::component::ComponentInspector::new(ui);
                inspector.inspect("Blag", &mut blag);
                */
            }
            EditorViewType::Scene => {
                self.views
                    .scene
                    .show(ui, &mut self.views.drag_drop, &mut self.vms.scene)
            }
            EditorViewType::EntityHierarchy => {
                self.views
                    .hierarchy
                    .show(ui, &mut self.views.drag_drop, &mut self.vms.scene);
            }
        }
    }

    #[inline(always)]
    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            EditorViewType::Assets => self.views.assets.title(),
            EditorViewType::Inspector => "Placeholder Inspector".into(),
            EditorViewType::Scene => self.views.scene.title(),
            EditorViewType::EntityHierarchy => self.views.hierarchy.title(),
        }
    }
}

impl ard_engine::render::renderer::gui::View for EditorGuiView {
    fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let input = res.get::<InputState>().unwrap();
        let assets = res.get::<Assets>().unwrap();
        let mut tree = res.get_mut::<EditorDockTree>().unwrap();
        let mut views = res.get_mut::<EditorViews>().unwrap();
        let mut models = res.get_mut::<EditorViewModels>().unwrap();

        // Drag drop state
        views
            .drag_drop
            .set_drag_state(ctx.memory().is_anything_being_dragged());

        // Toolbar
        egui::TopBottomPanel::top("MenuBar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        commands.events.submit(Stop);
                    }
                });
            });
        });

        egui::Window::new("TestWindow").show(ctx, |ui| {
            struct Blag {
                x: u32,
                y: f32,
            }

            impl crate::inspection::Inspectable for Blag {
                fn inspect(&mut self, inspector: &mut impl crate::inspection::Inspector) {
                    inspector.inspect_u32("x", &mut self.x);
                    inspector.inspect_f32("y", &mut self.y);
                }
            }

            let mut blag = Blag { x: 0, y: 0.0 };

            //let mut inspector = crate::inspection::component::ComponentInspector::new(ui);
            //inspector.inspect("Blag", &mut blag);
        });

        // Primary editor docking area
        DockArea::new(&mut tree.deref_mut().0)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(
                ctx,
                &mut EditorPresenter {
                    views: views.deref_mut(),
                    vms: models.deref_mut(),
                },
            );

        // Undo/redo
        if let Some((_, ty)) = tree.0.find_active_focused() {
            // TODO: Some imgui elements have an internal undo stack which we should replace with
            // this
            if input.key(Key::LCtrl) {
                if input.key_down(Key::Z) {
                    models.undo(*ty, &commands.entities, res);
                } else if input.key_down(Key::Y) {
                    models.redo(*ty, &commands.entities, res);
                }
            }
        }

        // Apply sent messages to each view model
        let mut settings = res.get_mut::<RendererSettings>().unwrap();

        models.assets.apply(&mut ());
        models.scene.apply(&mut SceneViewUpdate {
            dt: ctx.input().stable_dt,
            entity_commands: &commands.entities,
            assets: assets.deref(),
            settings: settings.deref_mut(),
            input: input.deref(),
        });

        // Reset drop state
        views.drag_drop.reset_on_drop();

        // Update canvas size
        settings.canvas_size = Some(models.scene.vm.view_size);
    }
}
