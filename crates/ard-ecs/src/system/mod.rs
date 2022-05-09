pub mod data;
pub mod handler;
pub mod query;

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    ops::{Add, AddAssign},
};

use self::handler::EventHandler;
use crate::{
    component::filter::ComponentFilter,
    dispatcher::EventSender,
    id_map::TypeIdMap,
    key::TypeKey,
    prelude::{EntityCommands, Event},
    resource::filter::ResourceFilter,
    system::{data::SystemData, query::QueryGenerator},
    tag::filter::TagFilter,
    world::entities::Entities,
};

pub struct Context<'a, S: SystemState> {
    pub queries: QueryGenerator<'a>,
    pub resources: <S::Resources as ResourceFilter>::Set,
    pub commands: EntityCommands,
    pub events: EventSender,
    pub entities: Option<&'a mut Entities>,
}

pub struct System {
    pub(crate) state: Box<dyn SystemStateExt>,
    pub(crate) exclusive: bool,
    pub(crate) entities: bool,
    /// Maps ID's of events to the handlers that handle them.
    pub(crate) handlers: TypeIdMap<Box<dyn EventHandler>>,
}

pub struct SystemBuilder<S: SystemState> {
    state: S,
    handlers: TypeIdMap<Box<dyn EventHandler>>,
}

#[derive(Default, Clone)]
pub struct SystemDataAccesses {
    pub read_components: TypeKey,
    pub mut_components: TypeKey,
    pub read_tags: TypeKey,
    pub mut_tags: TypeKey,
    pub read_resources: TypeKey,
    pub mut_resources: TypeKey,
}

/// A system is the logical component of the ECS. A system operates on a subset of components,
/// a subset of optional tags, and a set of resources.
pub trait SystemState: Send + Sync {
    /// Request that the system run on the main thread.
    const EXCLUSIVE: bool = false;

    /// Request that the system be granted access to the `Entities` object of the active world.
    const ENTITIES: bool = false;

    /// Tags and components required by the system.
    type Data: SystemData;

    /// Resources the system requests access to.
    type Resources: ResourceFilter;
}

/// # Note
/// This trait is automatically implemented for all systems. Do NOT manually implement this trait.
pub trait SystemStateExt: Send + Sync {
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn accesses(&self) -> SystemDataAccesses;
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
        }
    }

    pub fn with_handler<E: 'static + Event>(
        mut self,
        handler: fn(&mut S, Context<S>, E) -> (),
    ) -> Self {
        self.handlers.insert(TypeId::of::<E>(), Box::new(handler));
        self
    }

    pub fn build(self) -> System {
        System {
            state: Box::new(self.state),
            exclusive: S::EXCLUSIVE,
            entities: S::ENTITIES,
            handlers: self.handlers,
        }
    }
}

impl<T: SystemState + 'static> SystemStateExt for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn accesses(&self) -> SystemDataAccesses {
        SystemDataAccesses {
            mut_components: <T::Data as SystemData>::Components::mut_type_key(),
            read_components: <T::Data as SystemData>::Components::read_type_key(),
            mut_tags: <T::Data as SystemData>::Tags::mut_type_key(),
            read_tags: <T::Data as SystemData>::Tags::read_type_key(),
            mut_resources: T::Resources::mut_type_key(),
            read_resources: T::Resources::read_type_key(),
        }
    }
}

impl SystemDataAccesses {
    /// Determines if a system with self's accesses can run in parallel with the accesses of other.
    pub fn compatible_with(&self, other: &SystemDataAccesses) -> bool {
        let mut all_other_components = other.mut_components.clone();
        all_other_components += other.read_components.clone();

        let mut all_other_tags = other.mut_tags.clone();
        all_other_tags += other.read_tags.clone();

        let mut all_other_resources = other.mut_resources.clone();
        all_other_resources += other.read_resources.clone();

        self.mut_components.disjoint(&all_other_components)
            && self.read_components.disjoint(&other.mut_components)
            && self.mut_tags.disjoint(&all_other_tags)
            && self.read_tags.disjoint(&other.mut_tags)
            && self.mut_resources.disjoint(&all_other_resources)
            && self.read_resources.disjoint(&other.mut_resources)
    }
}

impl Add for SystemDataAccesses {
    type Output = SystemDataAccesses;

    fn add(self, rhs: Self) -> Self::Output {
        SystemDataAccesses {
            mut_components: self.mut_components + rhs.mut_components,
            read_components: self.read_components + rhs.read_components,
            mut_tags: self.mut_tags + rhs.mut_tags,
            read_tags: self.read_tags + rhs.read_tags,
            mut_resources: self.mut_resources + rhs.mut_resources,
            read_resources: self.read_resources + rhs.read_resources,
        }
    }
}

impl AddAssign for SystemDataAccesses {
    fn add_assign(&mut self, rhs: Self) {
        self.mut_components += rhs.mut_components.clone();
        self.read_components += rhs.read_components.clone();
        self.mut_tags += rhs.mut_tags.clone();
        self.read_tags += rhs.read_tags.clone();
        self.mut_resources += rhs.mut_resources.clone();
        self.read_resources += rhs.read_resources;
    }
}
