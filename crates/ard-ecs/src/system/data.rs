use crate::{
    component::{filter::ComponentFilter, Component},
    entity::Entity,
    prelude::Read,
    system::query::{ComponentQuery, EntityComponentQuery, EntityComponentTagQuery, Query},
    tag::filter::TagFilter,
};

use super::query::{SingleComponentQuery, SingleComponentTagQuery, SingleQuery};

/// Represents the data a system requests.
pub trait SystemData: Sized {
    /// This system data requires exclusive access to all components and tags.
    const EVERYTHING: bool;

    /// Required components.
    type Components: ComponentFilter;

    /// Optional tags.
    type Tags: TagFilter;

    /// Query for a single entities components and tags.
    type SingleQuery: SingleQuery<Self::Components, Self::Tags>;

    /// Query associated with the system data.
    type Query: Query<Self::Components, Self::Tags>;
}

#[derive(Component)]
pub struct DummyComponent;

/// This system data indicates that the query may access any and all components and tags, or that
/// a system may access all resources.
pub struct Everything;

impl SystemData for Everything {
    const EVERYTHING: bool = true;
    type Components = (Read<DummyComponent>,);
    type Tags = ();
    type Query = ();
    type SingleQuery = ();
}

impl SystemData for () {
    const EVERYTHING: bool = false;
    type Components = (Read<DummyComponent>,);
    type Tags = ();
    type Query = ();
    type SingleQuery = ();
}

/// Implementation for just components.
impl<C: ComponentFilter> SystemData for C {
    const EVERYTHING: bool = false;
    type Components = C;
    type Tags = ();
    type Query = ComponentQuery<Self::Components>;
    type SingleQuery = SingleComponentQuery<Self::Components>;
}

/// Implementation for components and their entity.
impl<C: ComponentFilter> SystemData for (Entity, C) {
    const EVERYTHING: bool = false;
    type Components = C;
    type Tags = ();
    type Query = EntityComponentQuery<Self::Components>;
    type SingleQuery = SingleComponentQuery<Self::Components>;
}

/// Implement for components, entities, and tags
impl<C: ComponentFilter, T: TagFilter> SystemData for (Entity, C, T) {
    const EVERYTHING: bool = false;
    type Components = C;
    type Tags = T;
    type Query = EntityComponentTagQuery<Self::Components, Self::Tags>;
    type SingleQuery = SingleComponentTagQuery<Self::Components, Self::Tags>;
}
