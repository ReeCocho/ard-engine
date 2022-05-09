pub mod stage;

use crossbeam_channel::{unbounded, Receiver, Sender};
use rayon::prelude::*;

use crate::{
    dispatcher::stage::Stage,
    prelude::{Event, EventExt, System},
    resource::Resources,
    world::World,
};

/// A dispatcher is a collection of systems that run in a particular order (or parallel).
pub struct Dispatcher {
    stages: Vec<Stage>,
    event_receiver: Receiver<Box<dyn EventExt>>,
    event_sender: Sender<Box<dyn EventExt>>,
}

#[derive(Clone)]
pub struct EventSender {
    sender: Sender<Box<dyn EventExt>>,
}

impl Default for Dispatcher {
    fn default() -> Self {
        let (event_sender, event_receiver) = unbounded();
        Dispatcher {
            stages: vec![Stage::default()],
            event_sender,
            event_receiver,
        }
    }
}

impl Dispatcher {
    pub fn new() -> Self {
        Dispatcher::default()
    }

    /// Gets the stages in the dispatcher.
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// Adds a system to the dispatcher.
    pub fn add_system(&mut self, system: impl Into<System>) {
        if let Some(system) = self.stages.last_mut().unwrap().add_system(system.into()) {
            let mut stage = Stage::default();
            stage.add_system(system);
            self.stages.push(stage);
        }
    }

    /// Submits an event to the dispatcher.
    #[inline]
    pub fn submit(&self, evt: impl Event + 'static) {
        self.event_sender
            .send(Box::new(evt))
            .expect("unable to submit event");
    }

    #[inline]
    pub fn event_sender(&self) -> EventSender {
        EventSender {
            sender: self.event_sender.clone(),
        }
    }

    /// Runs all systems within the dispatcher until the event queue is empty.
    pub fn run(&mut self, world: &mut World, resources: &Resources) {
        for event in self.event_receiver.try_iter() {
            let event_id = event.as_ref().as_any().type_id();

            world.process_entities();

            for stage in &mut self.stages {
                let events = EventSender {
                    sender: self.event_sender.clone(),
                };
                let main = &mut stage.main;
                let parallel = &mut stage.parallel;
                let tags = &world.tags;
                let archetypes = &world.archetypes;
                let commands = world.entities.commands();
                let entities = &mut world.entities;
                let evt = &event;

                // NOTE: `Exclusive` systems are required to run on the main thread. Conveniently,
                // argument A to join will run on the main thread and argumetn B will be parallel,
                // so this is fine.
                rayon::join(
                    || {
                        if let Some(system) = main {
                            if let Some(handler) = system.handlers.get(&event_id) {
                                handler.handle(
                                    system.state.as_mut(),
                                    tags,
                                    archetypes,
                                    commands.clone(),
                                    events.clone(),
                                    Some(entities),
                                    resources,
                                    evt.as_ref(),
                                );
                            }
                        }
                    },
                    || {
                        parallel
                            .iter_mut()
                            // Filter before parallelization so that we don't waste time context
                            // switching for a system that doesn't actually care about the event
                            .filter(|system| system.handlers.get(&event_id).is_some())
                            .par_bridge()
                            .into_par_iter()
                            .for_each(|system| {
                                system.handlers.get(&event_id).unwrap().handle(
                                    system.state.as_mut(),
                                    tags,
                                    archetypes,
                                    commands.clone(),
                                    events.clone(),
                                    None,
                                    resources,
                                    evt.as_ref(),
                                );
                            });
                    },
                );
            }
        }
    }
}

impl EventSender {
    /// Submit an event to the dispatcher.
    #[inline]
    pub fn submit(&self, evt: impl Event + 'static) {
        self.sender
            .send(Box::new(evt))
            .expect("unable to submit event");
    }
}
