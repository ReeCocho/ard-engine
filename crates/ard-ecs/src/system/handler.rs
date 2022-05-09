use crate::{
    archetype::Archetypes,
    dispatcher::EventSender,
    prelude::{
        Entities, EntityCommands, Event, EventExt, QueryGenerator, ResourceFilter, Resources,
    },
    tag::Tags,
};

use super::{Context, SystemState, SystemStateExt};

pub trait EventHandler: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn handle(
        &self,
        state: &mut dyn SystemStateExt,
        tags: &Tags,
        archetypes: &Archetypes,
        commands: EntityCommands,
        events: EventSender,
        entities: Option<&mut Entities>,
        resources: &Resources,
        event: &dyn EventExt,
    );
}

impl<S: 'static + SystemStateExt + SystemState, E: 'static + Event> EventHandler
    for fn(&mut S, Context<S>, E) -> ()
{
    fn handle(
        &self,
        state: &mut dyn SystemStateExt,
        tags: &Tags,
        archetypes: &Archetypes,
        commands: EntityCommands,
        events: EventSender,
        entities: Option<&mut Entities>,
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

        let ctx = Context {
            queries: QueryGenerator::new::<S::Data>(tags, archetypes),
            resources: <S::Resources as ResourceFilter>::get(resources),
            entities,
            events,
            commands,
        };

        self(state, ctx, event);
    }
}
