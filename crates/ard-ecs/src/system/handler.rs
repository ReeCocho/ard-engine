use crate::{
    archetype::Archetypes,
    component::filter::ComponentFilter,
    dispatcher::Events,
    key::TypeKey,
    prelude::{EntityCommands, Event, EventExt, Queries, ResourceFilter, Resources},
    resource::res::Res,
    tag::{filter::TagFilter, Tags},
};

use super::{commands::Commands, data::SystemData, SystemState, SystemStateExt};

/// Describes the data accesses of a handler.
pub struct HandlerAccesses {
    pub all_components: TypeKey,
    pub read_components: TypeKey,
    pub write_components: TypeKey,
    pub all_tags: TypeKey,
    pub read_tags: TypeKey,
    pub write_tags: TypeKey,
    pub all_resources: TypeKey,
    pub read_resources: TypeKey,
    pub write_resources: TypeKey,
}

pub trait EventHandler: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn handle(
        &self,
        state: &mut dyn SystemStateExt,
        tags: &Tags,
        archetypes: &Archetypes,
        commands: EntityCommands,
        events: Events,
        resources: &Resources,
        event: &dyn EventExt,
    );

    fn accesses(&self) -> HandlerAccesses;
}

impl<
        S: 'static + SystemStateExt + SystemState,
        E: 'static + Event,
        C: SystemData,
        R: ResourceFilter,
    > EventHandler for fn(&mut S, E, Commands, Queries<C>, Res<R>) -> ()
{
    fn handle(
        &self,
        state: &mut dyn SystemStateExt,
        tags: &Tags,
        archetypes: &Archetypes,
        entities: EntityCommands,
        events: Events,
        resources: &Resources,
        event: &dyn EventExt,
    ) {
        let state = state
            .as_any_mut()
            .downcast_mut::<S>()
            .expect("event handler given incorrect system state type");

        let event = event
            .as_any()
            .downcast_ref::<E>()
            .expect("event handler given incorrect event type")
            .clone();

        let commands = Commands { entities, events };

        self(
            state,
            event,
            commands,
            Queries::new(tags, archetypes),
            Res::new(resources),
        );
    }

    fn accesses(&self) -> HandlerAccesses {
        HandlerAccesses {
            all_components: <C::Components as ComponentFilter>::type_key(),
            read_components: <C::Components as ComponentFilter>::read_type_key(),
            write_components: <C::Components as ComponentFilter>::mut_type_key(),
            all_tags: <C::Tags as TagFilter>::type_key(),
            read_tags: <C::Tags as TagFilter>::read_type_key(),
            write_tags: <C::Tags as TagFilter>::mut_type_key(),
            all_resources: R::type_key(),
            read_resources: R::read_type_key(),
            write_resources: R::mut_type_key(),
        }
    }
}

impl HandlerAccesses {
    /// Returns `true` if this access does not access data in an incompatible with another.
    #[inline]
    pub fn compatible(&self, other: &HandlerAccesses) -> bool {
        self.all_components.disjoint(&other.write_components)
            && self.all_resources.disjoint(&other.write_resources)
            && self.all_tags.disjoint(&other.write_tags)
    }
}
