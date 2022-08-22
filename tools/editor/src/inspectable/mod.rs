pub mod implementations;

use paste::paste;
use std::{any::Any, ptr::NonNull};

use crate::{controller::Command, editor::Resources};
use ard_engine::{
    ecs::prelude::*,
    game::{
        object::{empty::EmptyObject, static_object::StaticObject},
        SceneGameObject,
    },
    math::*,
};

use self::implementations::InspectGameObject;

pub trait Inspectable {
    fn inspect(&mut self, state: &mut InspectState);
}

pub trait Reflectable: Any + Clone + Send + Sync {
    fn get(&self, name: &str) -> Box<dyn Any>;

    fn set(&mut self, name: &str, value: Box<dyn Any>) -> Option<ValueDelta>;
}

pub struct InspectComponent<'a, C: Component + Inspectable + 'static> {
    entity: Entity,
    name: &'a str,
    phantom: std::marker::PhantomData<C>,
}

pub struct ValueDelta {
    pub old: Box<dyn Any + Send>,
    pub new: Box<dyn Any + Send>,
}

pub struct InspectState<'a, 'b> {
    pub resources: &'a mut Resources<'b>,
    pub entity: Entity,
    object_type: SceneGameObject,
    path: ValuePath,
    ty: InspectTy<'a>,
}

enum InspectTy<'a> {
    Inspection {
        ui: &'a imgui::Ui,
        queue: ModifyQueue,
    },
    UndoRedo {
        cmd: Option<&'a ModifyCommand>,
        is_undo: bool,
    },
}

#[derive(Default)]
pub struct ModifyQueue {
    commands: Vec<ModifyCommand>,
}

pub struct ModifyCommand {
    pub delta: ValueDelta,
    pub path: ValuePath,
    pub entity: Entity,
    pub object_type: SceneGameObject,
}

#[derive(Default, Clone, PartialEq, Eq)]
pub struct ValuePath {
    elements: Vec<ValuePathElement>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ValuePathElement {
    Field(String),
    ArrayElement(usize),
}

impl<'a, 'b> InspectState<'a, 'b> {
    pub fn new_inspection(
        resources: &'a mut Resources<'b>,
        entity: Entity,
        object_type: SceneGameObject,
        ui: &'a imgui::Ui,
    ) -> Self {
        Self {
            resources,
            entity,
            object_type,
            path: ValuePath::default(),
            ty: InspectTy::Inspection {
                ui,
                queue: ModifyQueue::default(),
            },
        }
    }

    pub fn new_undo_redo(
        resources: &'a mut Resources<'b>,
        entity: Entity,
        object_type: SceneGameObject,
        cmd: &'a ModifyCommand,
        is_undo: bool,
    ) -> Self {
        Self {
            resources,
            entity,
            object_type,
            path: ValuePath::default(),
            ty: InspectTy::UndoRedo {
                cmd: Some(cmd),
                is_undo,
            },
        }
    }

    pub fn inspect(&mut self) {
        match self.object_type {
            SceneGameObject::StaticObject => StaticObject::inspect(self.entity, self),
            SceneGameObject::EmptyObject => EmptyObject::inspect(self.entity, self),
        }
    }

    pub fn into_modify_queue(self) -> Option<ModifyQueue> {
        match self.ty {
            InspectTy::Inspection { queue, .. } => Some(queue),
            _ => None,
        }
    }
}

impl ModifyQueue {
    #[inline]
    pub fn add(&mut self, command: ModifyCommand) {
        self.commands.push(command);
    }

    #[inline]
    pub fn drain(&mut self) -> Vec<ModifyCommand> {
        std::mem::take(&mut self.commands)
    }
}

impl ValuePath {
    #[inline]
    pub fn push(&mut self, element: ValuePathElement) {
        self.elements.push(element);
    }

    #[inline]
    pub fn pop(&mut self) -> Option<ValuePathElement> {
        self.elements.pop()
    }

    #[inline]
    pub fn peek(&mut self) -> Option<&mut ValuePathElement> {
        self.elements.last_mut()
    }
}

impl Command for ModifyCommand {
    fn undo(&mut self, resc: &mut Resources) {
        let mut inspect_state =
            InspectState::new_undo_redo(resc, self.entity, self.object_type, self, true);

        inspect_state.inspect();
    }

    fn redo(&mut self, resc: &mut Resources) {
        let mut inspect_state =
            InspectState::new_undo_redo(resc, self.entity, self.object_type, self, false);

        inspect_state.inspect();
    }

    fn apply(&mut self, resc: &mut Resources) {}
}

impl<'a, 'b> InspectState<'a, 'b> {
    pub fn field<I: Inspectable>(&mut self, name: &str, data: &mut I) {
        self.path.push(ValuePathElement::Field(String::from(name)));
        data.inspect(self);
        self.path.pop();
    }
}

impl<'a, C: Component + Inspectable + 'static> InspectComponent<'a, C> {
    pub fn new(name: &'a str, entity: Entity) -> Self {
        Self {
            name,
            entity,
            phantom: Default::default(),
        }
    }
}

impl<'a, C: Component + Inspectable + 'static> Inspectable for InspectComponent<'a, C> {
    fn inspect(&mut self, state: &mut InspectState) {
        state
            .path
            .push(ValuePathElement::Field(String::from(self.name)));
        if let Some(mut component) = state.resources.queries.get::<Write<C>>(self.entity) {
            match &mut state.ty {
                InspectTy::Inspection { ui, .. } => {
                    if ui.collapsing_header(self.name, imgui::TreeNodeFlags::empty()) {
                        component.inspect(state);
                    }
                }
                InspectTy::UndoRedo { .. } => {
                    component.inspect(state);
                }
            }
        }
        state.path.pop();
    }
}

macro_rules! inspectable_input_impl {
    ($ty:ident $func:ident) => {
        impl Inspectable for $ty {
            fn inspect(&mut self, state: &mut InspectState) {
                match &mut state.ty {
                    InspectTy::Inspection { ui, queue } => {
                        let field_name = state.path.peek().unwrap();
                        match field_name {
                            ValuePathElement::Field(name) => {
                                let old = *self;
                                if ui.$func(name, self).no_undo_redo(true).build() {
                                    let new = *self;
                                    queue.add(ModifyCommand {
                                        delta: ValueDelta {
                                            new: Box::new(new),
                                            old: Box::new(old),
                                        },
                                        entity: state.entity,
                                        object_type: state.object_type,
                                        path: state.path.clone(),
                                    });
                                }
                            }
                            _ => todo!(),
                        }
                    }
                    InspectTy::UndoRedo { cmd, is_undo } => {
                        if let Some(inner_cmd) = cmd {
                            if inner_cmd.path == state.path {
                                if *is_undo {
                                    let value = inner_cmd.delta.old.downcast_ref().unwrap();
                                    *self = *value;
                                } else {
                                    let value = inner_cmd.delta.new.downcast_ref().unwrap();
                                    *self = *value;
                                }
                                *cmd = None;
                            }
                        }
                    }
                }
            }
        }
    };
}

inspectable_input_impl!(f32 input_float);
inspectable_input_impl!(Vec2 input_float2);
inspectable_input_impl!(Vec3 input_float3);
inspectable_input_impl!(Vec3A input_float3);
inspectable_input_impl!(Vec4 input_float4);
