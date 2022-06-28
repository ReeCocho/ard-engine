pub mod commands;
pub mod data;
pub mod handler;
pub mod query;

use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use self::{commands::Commands, handler::EventHandler};
use crate::{
    id_map::TypeIdMap,
    prelude::Event,
    resource::{filter::ResourceFilter, res::Res},
    system::{data::SystemData, query::Queries},
};

pub struct System {
    pub(crate) state: Box<dyn SystemStateExt>,
    /// Type ID of the system state.
    pub(crate) id: TypeId,
    pub(crate) main_thread: bool,
    /// Maps ID's of events to the handlers that handle them.
    pub(crate) handlers: TypeIdMap<Box<dyn EventHandler>>,
    // Which systems must run before/after this one for a particular event type. The first value in
    // the tuple is the event type. The second is the system state type. If the mapped value is
    // `true`, the system will not run if the associated system does not exist. Otherwise, it will
    // run and behave as if the system is already completed.
    pub(crate) run_before: HashMap<(TypeId, TypeId), bool>,
    pub(crate) run_after: HashMap<(TypeId, TypeId), bool>,
}

pub struct SystemBuilder<S: SystemState> {
    state: S,
    handlers: TypeIdMap<Box<dyn EventHandler>>,
    pub(crate) run_before: HashMap<(TypeId, TypeId), bool>,
    pub(crate) run_after: HashMap<(TypeId, TypeId), bool>,
}

/// A system is the logical component of the ECS. A system operates on a subset of components,
/// a subset of optional tags, and a set of resources.
pub trait SystemState: Send {
    /// Request that the system run on the main thread.
    const MAIN_THREAD: bool = false;

    /// Name of the system for debugging purposes.
    const DEBUG_NAME: &'static str;

    #[inline]
    fn debug_name() -> &'static str {
        Self::DEBUG_NAME
    }
}

/// # Note
/// This trait is automatically implemented for all systems. Do NOT manually implement this trait.
pub trait SystemStateExt: Send {
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn debug_name(&self) -> &'static str;
}

impl System {
    /// Get the handler for the given event type.
    pub fn handler<E: Event + 'static>(&self) -> Option<&dyn EventHandler> {
        self.handler_by_id(TypeId::of::<E>())
    }

    /// Get the handler for the given event type ID.
    pub fn handler_by_id(&self, id: TypeId) -> Option<&dyn EventHandler> {
        self.handlers.get(&id).map(|e| e.as_ref())
    }
}

impl<S: SystemState + 'static> SystemBuilder<S> {
    pub fn new(state: S) -> Self {
        Self {
            state,
            handlers: HashMap::default(),
            run_after: HashMap::default(),
            run_before: HashMap::default(),
        }
    }

    /// Adds an event handler for the state.
    pub fn with_handler<
        E: 'static + Event,
        C: 'static + SystemData,
        R: 'static + ResourceFilter,
    >(
        mut self,
        handler: fn(&mut S, E, Commands, Queries<C>, Res<R>) -> (),
    ) -> Self {
        self.handlers.insert(TypeId::of::<E>(), Box::new(handler));
        self
    }

    /// Indicates that this system should run after another system for the given event type. If
    /// the system type doesn't handle the associated event, or the system hasn't been added, this
    /// system will still run.
    pub fn run_after<E: 'static + Event, OtherS: SystemState + 'static>(mut self) -> Self {
        let key = (TypeId::of::<E>(), TypeId::of::<OtherS>());
        assert!(!self.run_after.contains_key(&key));
        assert!(!self.run_before.contains_key(&key));
        self.run_after.insert(key, false);
        self
    }

    /// Indicates that this system should run after another system for the given event type. If
    /// the system type doesn't handle the associated event, or the system hasn't been added, this
    /// system will NOT run.
    pub fn run_after_req<E: 'static + Event, OtherS: SystemState + 'static>(mut self) -> Self {
        let key = (TypeId::of::<E>(), TypeId::of::<OtherS>());
        assert!(!self.run_after.contains_key(&key));
        assert!(!self.run_before.contains_key(&key));
        self.run_after.insert(key, true);
        self
    }

    /// Indicates that this system should run before another system for the given event type. If
    /// the system type doesn't handle the associated event, or the system hasn't been added, this
    /// system will still run.
    pub fn run_before<E: 'static + Event, OtherS: SystemState + 'static>(mut self) -> Self {
        let key = (TypeId::of::<E>(), TypeId::of::<OtherS>());
        assert!(!self.run_after.contains_key(&key));
        assert!(!self.run_before.contains_key(&key));
        self.run_before.insert(key, false);
        self
    }

    /// Indicates that this system should run before another system for the given event type. If
    /// the system type doesn't handle the associated event, or the system hasn't been added, this
    /// system will NOT run.
    pub fn run_before_req<E: 'static + Event, OtherS: SystemState + 'static>(mut self) -> Self {
        let key = (TypeId::of::<E>(), TypeId::of::<OtherS>());
        assert!(!self.run_after.contains_key(&key));
        assert!(!self.run_before.contains_key(&key));
        self.run_before.insert(key, true);
        self
    }

    pub fn build(self) -> System {
        System {
            state: Box::new(self.state),
            id: TypeId::of::<S>(),
            main_thread: S::MAIN_THREAD,
            handlers: self.handlers,
            run_after: self.run_after,
            run_before: self.run_before,
        }
    }
}

impl<T: SystemState + 'static> SystemStateExt for T {
    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline]
    fn debug_name(&self) -> &'static str {
        T::DEBUG_NAME
    }
}
