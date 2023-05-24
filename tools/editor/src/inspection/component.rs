use ard_engine::{
    ecs::prelude::{Component, Entity, Everything, Queries, Write},
    math::Vec3,
};

use super::{Inspectable, Inspector};

pub struct ComponentInspector<'a, C> {
    ui: &'a mut egui::Ui,
    entity: Entity,
    _phantom: std::marker::PhantomData<C>,
}

struct ComponentInspectorImpl<'a, C> {
    ui: &'a mut egui::Ui,
    entity: Entity,
    modifications: Vec<ComponentModification>,
    _phantom: std::marker::PhantomData<C>,
}

pub struct ComponentModification {
    pub undo: Box<ModifyFn>,
    pub redo: Box<ModifyFn>,
}

type ModifyFn = dyn FnMut(Queries<Everything>) -> ();

impl<'a, C: Inspectable + Component + 'static> ComponentInspector<'a, C> {
    pub fn new(ui: &'a mut egui::Ui, entity: Entity) -> Self {
        Self {
            ui,
            entity,
            _phantom: Default::default(),
        }
    }

    pub fn inspect(&mut self, name: &str, component: &mut C) -> Vec<ComponentModification> {
        let mut modifications = Vec::default();
        egui::Grid::new(name)
            .num_columns(2)
            .spacing([40.0, 4.0])
            .striped(true)
            .show(self.ui, |ui| {
                ui.heading(name);
                ui.vertical_centered_justified(|ui| {
                    ui.heading("");
                });
                ui.end_row();

                let mut inspector = ComponentInspectorImpl::<C> {
                    ui,
                    entity: self.entity,
                    modifications: Vec::default(),
                    _phantom: Default::default(),
                };
                component.inspect(&mut inspector);
                modifications = inspector.modifications;
            });

        modifications
    }
}

macro_rules! make_modify {
    ($name:expr, $entity:expr, $old:expr, $new:expr, $c:ty, $ur:ty) => {{
        let entity = $entity;
        let undo_name = String::from($name);
        let redo_name = String::from($name);

        ComponentModification {
            undo: Box::new(move |queries| {
                let mut component = queries.get::<Write<$c>>(entity).unwrap();
                component.inspect(&mut <$ur>::new(&undo_name, true, &$old, &$new));
            }),
            redo: Box::new(move |queries| {
                let mut component = queries.get::<Write<$c>>(entity).unwrap();
                component.inspect(&mut <$ur>::new(&redo_name, true, &$old, &$new));
            }),
        }
    }};
}

impl<C: Inspectable + Component + 'static> Inspector for ComponentInspectorImpl<'_, C> {
    fn inspect_u32(&mut self, name: &str, data: &mut u32) {
        self.ui.label(name);

        let mut temp = *data;
        let old = temp;
        let changed = self.ui.add(egui::DragValue::new(&mut temp)).changed();
        let new = temp;

        if changed {
            *data = new;
            self.modifications
                .push(make_modify!(name, self.entity, old, new, C, UndoRedou32));
        }

        self.ui.end_row();
    }

    fn inspect_f32(&mut self, name: &str, data: &mut f32) {
        self.ui.label(name);

        let mut temp = *data;
        let old = temp;
        let changed = self.ui.add(egui::DragValue::new(&mut temp)).changed();
        let new = temp;

        if changed {
            *data = new;
            self.modifications
                .push(make_modify!(name, self.entity, old, new, C, UndoRedof32));
        }

        self.ui.end_row();
    }

    fn inspect_vec3(&mut self, name: &str, data: &mut Vec3) {
        self.ui.label(name);
        self.ui.label(name);
        self.ui.end_row();
    }
}

macro_rules! undo_redo_impl {
    ($t:ty, $f:ident) => {
        paste::paste! {
            struct [<UndoRedo $t>]<'a> {
                name: &'a str,
                undo: bool,
                old: &'a $t,
                new: &'a $t,
            }

            impl<'a> [<UndoRedo $t>]<'a> {
                pub fn new(name: &'a str, undo: bool, old: &'a $t, new: &'a $t) -> Self {
                    Self {
                        name,
                        undo,
                        old,
                        new,
                    }
                }
            }

            impl<'a> Inspector for [<UndoRedo $t>]<'a> {
                fn $f(&mut self, name: &str, data: &mut $t) {
                    if name != self.name {
                        return;
                    }

                    if self.undo {
                        *data = self.old.clone();
                    } else {
                        *data = self.new.clone();
                    }
                }
            }
        }
    };
}

undo_redo_impl!(u32, inspect_u32);
undo_redo_impl!(f32, inspect_f32);
undo_redo_impl!(Vec3, inspect_vec3);
