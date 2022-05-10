use crate::{
    component::{filter::ComponentFilter, Component},
    entity::Entity,
    prelude::Read,
    system::query::{ComponentQuery, EntityComponentQuery, EntityComponentTagQuery, Query},
    tag::filter::TagFilter,
    world::World,
};

/// Represents the data a system requests.
pub trait SystemData: Sized {
    /// Required components.
    type Components: ComponentFilter;

    /// Optional tags.
    type Tags: TagFilter;

    /// Query associated with the system data.
    type Query: Query<Self::Components, Self::Tags>;

    /// Constructs the query associted with the system data given a world to operate on.
    fn make_query(world: &World) -> Self::Query {
        Self::Query::new(world.tags(), world.archetypes())
    }
}

#[derive(Component)]
pub struct DummyComponent;

impl SystemData for () {
    type Components = (Read<DummyComponent>,);
    type Tags = ();
    type Query = ();
}

/// Implementation for just components.
impl<C: ComponentFilter> SystemData for C {
    type Components = C;
    type Tags = ();
    type Query = ComponentQuery<Self::Components>;

    fn make_query(world: &World) -> Self::Query {
        Self::Query::new(world.tags(), world.archetypes())
    }
}

/// Implementation for components and their entity.
impl<C: ComponentFilter> SystemData for (Entity, C) {
    type Components = C;
    type Tags = ();
    type Query = EntityComponentQuery<Self::Components>;

    fn make_query(world: &World) -> Self::Query {
        Self::Query::new(world.tags(), world.archetypes())
    }
}

/// Implement for components, entities, and tags
impl<C: ComponentFilter, T: TagFilter> SystemData for (Entity, C, T) {
    type Components = C;
    type Tags = T;
    type Query = EntityComponentTagQuery<Self::Components, Self::Tags>;

    fn make_query(world: &World) -> Self::Query {
        Self::Query::new(world.tags(), world.archetypes())
    }
}
