use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    hash::BuildHasherDefault,
    ops::{BitAnd, BitOr, Div, Not},
    ptr::NonNull,
};

use bitvec::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::{
    archetype::Archetypes,
    id_map::{FastIntHasher, TypeIdMap},
    prelude::{Entities, EntityCommands, Event, EventExt, System},
    resource::Resources,
    system::{
        handler::{EventHandler, HandlerAccesses},
        SystemStateExt,
    },
    tag::Tags,
    world::World,
};

/// Maximum number of systems allowed in a disaptcher.
pub const MAX_SYSTEMS: usize = 128;

/// Bits which represent a set of systems.
type SystemSet = BitArr!(for MAX_SYSTEMS);

/// A dispatcher is a collection of systems that run in a particular order (or parallel).
pub struct Dispatcher {
    /// All systems in the dispatcher.
    systems: Vec<System>,
    /// Thread pool for performing jobs.
    thread_pool: ThreadPool,
    /// Maps event types to the dispatcher state for that event type. If there is no entry, then
    /// no system handles the event type, so it can be ignored.
    event_to_systems: TypeIdMap<DispatcherState>,
    event_receiver: Receiver<Box<dyn EventExt>>,
    event_sender: Sender<Box<dyn EventExt>>,
}

#[derive(Default)]
pub struct DispatcherBuilder {
    /// Requested number of threads. `None` indicates it should be computed automatically.
    thread_count: Option<usize>,
    /// Initial events to submit.
    events: Vec<Box<dyn EventExt>>,
    /// Maps system state type IDs to the associated system object.
    systems: TypeIdMap<System>,
}

/// A description of the state of the dispatcher for a particular event.
#[derive(Default)]
struct DispatcherState {
    /// System states for the event.
    states: Vec<SystemState>,
    /// Maps system state type IDs to the index in the `states` vec where they exist.
    type_to_state: TypeIdMap<usize>,
    /// Maps each system to a `SystemSet` of compatible systems.
    compatibility: Vec<SystemSet>,
    /// Cache that maps a set of running systems and pending systems (in that order) with a subset
    /// of those systems that are actually compatible. Each bit in the `BitArr` represents a
    /// system.
    cache: HashMap<(SystemSet, SystemSet), Vec<usize>>,
    bron_kerbosch: Vec<SystemSet>,
    to_remove: Vec<usize>,
    pending: HashSet<usize, BuildHasherDefault<FastIntHasher>>,
    running: HashSet<usize, BuildHasherDefault<FastIntHasher>>,
}

/// A description of the state of a particular system for a particular event.
struct SystemState {
    /// Index of the system in the primary system list.
    system: usize,
    /// Indicates this system must run on the main thread.
    /// TODO: Not actually used yet.
    _main_thread: bool,
    /// Handler data acceses to check for compatibility.
    accesses: HandlerAccesses,
    /// Receiver that threads use to notify the main thread that the system has finished running.
    finished: Receiver<()>,
    /// Sender that threads use to notify the main thread that the system has finished running.
    thread_sender: Sender<()>,
    /// Number of dependencies this system has.
    dependency_count: usize,
    /// Number of dependencies the system is waiting on currently.
    waiting_on: usize,
    /// Indices of systems that are dependent on us.
    dependents: Vec<usize>,
}

/// Description for a thread of a system to run.
struct SystemPacket {
    /// System to run.
    system: NonNull<dyn SystemStateExt>,
    /// Handler to run the system with.
    handler: NonNull<dyn EventHandler>,
    /// Archetypes the system must use.
    archetypes: NonNull<Archetypes>,
    /// Tags the system must use.
    tags: NonNull<Tags>,
    /// Entities of the world.
    entities: NonNull<Entities>,
    /// Resources the systems must use.
    resources: NonNull<Resources>,
    event: NonNull<dyn EventExt>,
    /// Sender that threads use to notify the main thread that a system has finished running.
    thread_sender: Sender<()>,
    commands: EntityCommands,
    events: Events,
}

unsafe impl Send for SystemPacket {}
unsafe impl Sync for SystemPacket {}

/// Used to send events back to the dispatcher.
#[derive(Clone)]
pub struct Events {
    sender: Sender<Box<dyn EventExt>>,
}

impl Default for Dispatcher {
    fn default() -> Self {
        DispatcherBuilder::default().build()
    }
}

impl Dispatcher {
    pub fn builder() -> DispatcherBuilder {
        DispatcherBuilder::default()
    }

    /// Submits an event to the dispatcher.
    #[inline]
    pub fn submit(&self, evt: impl Event + 'static) {
        self.event_sender
            .send(Box::new(evt))
            .expect("unable to submit event");
    }

    #[inline]
    pub fn event_sender(&self) -> Events {
        Events {
            sender: self.event_sender.clone(),
        }
    }

    /// Runs all systems within the dispatcher until the event queue is empty.
    pub fn run(&mut self, world: &mut World, resources: &Resources) {
        for event in self.event_receiver.try_iter() {
            let event_id = event.as_ref().as_any().type_id();

            world.process_entities();

            if let Some(dispatcher_state) = self.event_to_systems.get_mut(&event_id) {
                let pending = &mut dispatcher_state.pending;
                let mut finished = 0;
                let running = &mut dispatcher_state.running;

                pending.clear();
                running.clear();

                // Setup: reset waiting counters. Systems with no dependencies are pending.
                for (i, system) in dispatcher_state.states.iter_mut().enumerate() {
                    system.waiting_on = system.dependency_count;

                    if system.dependency_count == 0 {
                        pending.insert(i);
                    }
                }

                // Loop until all systems have finished
                while finished != dispatcher_state.states.len() {
                    // Determine systems that have finished running
                    let to_remove = &mut dispatcher_state.to_remove;
                    to_remove.clear();

                    for system in running.iter() {
                        let idx = *system;

                        // Check to see if the system has finished running
                        if dispatcher_state.states[idx].finished.try_recv().is_err() {
                            continue;
                        }

                        to_remove.push(idx);
                        finished += 1;

                        // Notify dependencies of the completion
                        // NOTE: Borrow checker bullsh*t means we can't iterate over 'dependents'
                        // while modifying 'states' because of mutable/immutable borrow.
                        for i in 0..dispatcher_state.states[idx].dependents.len() {
                            let dependent_idx = dispatcher_state.states[idx].dependents[i];
                            let dependent = &mut dispatcher_state.states[dependent_idx];

                            dependent.waiting_on -= 1;

                            // Move to pending if we aren't waiting anymore
                            if dependent.waiting_on == 0 {
                                pending.insert(dependent_idx);
                            }
                        }
                    }

                    for idx in to_remove {
                        running.remove(idx);
                    }

                    // If there are no pending systems, we loop
                    if pending.is_empty() {
                        continue;
                    }

                    // Create system sets
                    let mut running_set: SystemSet = BitArray::ZERO;
                    let mut pending_set: SystemSet = BitArray::ZERO;

                    for idx in running.iter() {
                        running_set.set(*idx, true);
                    }

                    for idx in pending.iter() {
                        pending_set.set(*idx, true);
                    }

                    // Check if we've seen this combo already in the cache
                    let cache_set = (running_set, pending_set);
                    let to_run = if let Some(result) = dispatcher_state.cache.get(&cache_set) {
                        result
                    } else {
                        let max_cliques = &mut dispatcher_state.bron_kerbosch;
                        max_cliques.clear();

                        // It is currently 10:29 PM, 10/5/22. I have been beating my head against a
                        // brick wall trying to figure out why this algorithm is breaking for this
                        // particular example:
                        //
                        // A - B - C
                        //
                        // Where A and B form the running set and C forms the pending set. As it
                        // turns out, I am a fool who does not know how to read. The Bron-Kerbosch
                        // algorithm !!REQUIRES!! that each vertex in the pending set form a clique
                        // with !!ALL!! vertices in the running set.
                        //
                        // I have seen the light.
                        //
                        // This little loop guarantees that condition.
                        for i in pending_set.clone().iter_ones() {
                            let compatibility = dispatcher_state.compatibility[i];
                            if (compatibility & running_set) != running_set {
                                pending_set.set(i, false);
                            }
                        }

                        bron_kerbosch(
                            running_set,
                            pending_set,
                            BitArray::ZERO,
                            &dispatcher_state.compatibility,
                            max_cliques,
                        );

                        // Pick the maximum amongst all the maximal cliques
                        let mut result = if max_cliques.is_empty() {
                            BitArray::ZERO
                        } else {
                            // Find the maximum amongst all the cliques
                            let mut max = 0;
                            let mut max_len = max_cliques[0].count_ones();

                            for (i, clique) in max_cliques.iter().enumerate().skip(1) {
                                if clique.count_ones() > max_len {
                                    max = i;
                                    max_len = clique.count_ones();
                                }
                            }

                            max_cliques[max]
                        };

                        // Get rid of the running systems
                        result &= running_set.not();
                        let mut to_cache = Vec::with_capacity(result.count_ones());
                        for i in result.iter_ones() {
                            to_cache.push(i);
                        }

                        // Add to the cache
                        dispatcher_state.cache.insert(cache_set, to_cache);
                        dispatcher_state.cache.get(&cache_set).unwrap()
                    };

                    // Send all compatible systems to the thread pool
                    for system in to_run {
                        let idx = *system;

                        // Ignore if already running
                        if running.contains(&idx) {
                            continue;
                        }

                        running.insert(idx);
                        pending.remove(&idx);

                        // Grab the primary system
                        let primary_idx = dispatcher_state.states[idx].system;
                        let primary_sys = &self.systems[primary_idx];

                        let packet = unsafe {
                            SystemPacket {
                                system: NonNull::new_unchecked(primary_sys.state.as_ref()
                                    as *const _
                                    as *mut _),
                                archetypes: NonNull::new_unchecked(
                                    (&world.archetypes) as *const _ as *mut _,
                                ),
                                entities: NonNull::new_unchecked(
                                    (&world.entities) as *const _ as *mut _,
                                ),
                                thread_sender: dispatcher_state.states[idx].thread_sender.clone(),
                                handler: NonNull::new_unchecked(
                                    primary_sys.handlers.get(&event_id).unwrap().as_ref()
                                        as *const _ as *mut _,
                                ),
                                resources: NonNull::new_unchecked(resources as *const _ as *mut _),
                                tags: NonNull::new_unchecked((&world.tags) as *const _ as *mut _),
                                event: NonNull::new_unchecked(event.as_ref() as *const _ as *mut _),
                                commands: world.entities.commands().clone(),
                                events: Events {
                                    sender: self.event_sender.clone(),
                                },
                            }
                        };

                        self.thread_pool.spawn(move || unsafe {
                            // Move packet to the thread
                            let mut packet = packet;

                            packet.handler.as_mut().handle(
                                packet.system.as_mut(),
                                packet.tags.as_ref(),
                                packet.archetypes.as_ref(),
                                packet.entities.as_ref(),
                                packet.commands,
                                packet.events,
                                packet.resources.as_ref(),
                                packet.event.as_ref(),
                            );

                            // Notify the main thread that the system has completed
                            packet.thread_sender.send(()).unwrap();
                        });
                    }
                }
            }
        }
    }
}

impl DispatcherBuilder {
    pub fn new() -> Self {
        DispatcherBuilder::default()
    }

    pub fn thread_count(&mut self, threads: usize) -> &mut Self {
        assert_ne!(threads, 0);
        self.thread_count = Some(threads);
        self
    }

    pub fn add_system(&mut self, system: impl Into<System>) -> &mut Self {
        let system = system.into();
        let id = system.id;
        assert!(!self.systems.contains_key(&id));
        self.systems.insert(id, system);
        self
    }

    pub fn submit(&mut self, event: impl Event + 'static) -> &mut Self {
        self.events.push(Box::new(event));
        self
    }

    pub fn build(&mut self) -> Dispatcher {
        // Determine what events each system handles
        let mut system_to_event_handlers = HashMap::<TypeId, HashSet<TypeId>>::default();

        for (system_id, system) in &self.systems {
            let mut event_handlers = HashSet::default();

            for event_id in system.handlers.keys() {
                event_handlers.insert(*event_id);
            }

            system_to_event_handlers.insert(*system_id, event_handlers);
        }

        // Remove event handlers from systems that depend on other event handlers that don't exist.
        // This must be done in multiple passes because we might remove a system in one pass that
        // another system depends on.
        loop {
            let mut to_remove = Vec::default();

            for (system_id, system) in &self.systems {
                for ((event, other_system), must_exist) in &system.run_after {
                    // Don't care about it if this flag isn't set
                    if !(*must_exist) {
                        continue;
                    }

                    if let Some(other_handlers) = system_to_event_handlers.get(other_system) {
                        // The other system does not handle the event we care about. Remove this
                        // event handler because it is dependent
                        if !other_handlers.contains(event) {
                            to_remove.push((*system_id, *event));
                        }
                    }
                    // System does not exist. Remove this event handler because it is dependent
                    else {
                        to_remove.push((*system_id, *event));
                    }
                }
            }

            // Didn't remove anything so we are done here
            if to_remove.is_empty() {
                break;
            }

            // Remove systems
            for (system, event) in to_remove {
                if let Some(system) = self.systems.get_mut(&system) {
                    system.handlers.remove(&event);
                }
            }
        }

        let systems = std::mem::take(&mut self.systems)
            .into_values()
            .collect::<Vec<_>>();
        let mut event_to_systems: TypeIdMap<DispatcherState> = TypeIdMap::default();

        // First loop over every system to initialize dispatcher state objects.
        for (system_idx, system) in systems.iter().enumerate() {
            // Loop over every event handler for this system
            for (event_id, handler) in &system.handlers {
                // Intitialize the dispatcher state
                let dispatcher_state = event_to_systems.entry(*event_id).or_default();
                let (thread_sender, finished) = crossbeam_channel::bounded(1);

                dispatcher_state.states.push(SystemState {
                    system: system_idx,
                    _main_thread: system.main_thread,
                    finished,
                    thread_sender,
                    dependency_count: 0,
                    waiting_on: 0,
                    dependents: Vec::default(),
                    accesses: handler.accesses(),
                });

                dispatcher_state
                    .type_to_state
                    .insert(system.id, dispatcher_state.states.len() - 1);
            }
        }

        // Determine compatibility for each system for each event type
        for dispatcher_state in event_to_systems.values_mut() {
            for (i, system) in dispatcher_state.states.iter().enumerate() {
                let mut compatible: SystemSet = BitArray::ZERO;

                for (j, other_system) in dispatcher_state.states.iter().enumerate() {
                    // Write archetypes must not overlap (also, we are compatible with ourselves)
                    if (i != j) && !system.accesses.compatible(&other_system.accesses) {
                        continue;
                    }

                    compatible.set(j, true);
                }

                dispatcher_state.compatibility.push(compatible);
            }
        }

        // Intitialize "after" dependencies
        for system in &systems {
            for (event, other_system_id) in system.run_after.keys() {
                let dispatcher_state = event_to_systems.get_mut(event).unwrap();
                let our_idx = *dispatcher_state.type_to_state.get(&system.id).unwrap();
                let other_idx = *dispatcher_state.type_to_state.get(other_system_id).unwrap();

                // Increment our dependency count
                dispatcher_state.states[our_idx].dependency_count += 1;

                // Notify the other system that we are a dependent
                dispatcher_state.states[other_idx].dependents.push(our_idx);
            }
        }

        // Intitialize "before" dependencies
        for system in &systems {
            for (event, other_system_id) in system.run_before.keys() {
                let dispatcher_state = event_to_systems.get_mut(event).unwrap();
                let our_idx = *dispatcher_state.type_to_state.get(&system.id).unwrap();
                let other_idx = *dispatcher_state.type_to_state.get(other_system_id).unwrap();

                // Ensure the other system is not already dependent on us.
                if dispatcher_state.states[our_idx]
                    .dependents
                    .contains(&other_idx)
                {
                    continue;
                }

                // The other system is a dependent of us
                dispatcher_state.states[our_idx].dependents.push(other_idx);

                // The other system gets an implicit dependency
                dispatcher_state.states[other_idx].dependency_count += 1;
            }
        }

        // Check for circular dependencies. This would imply that all systems have a dependency
        for dispatcher_state in event_to_systems.values() {
            let mut circular = !dispatcher_state.states.is_empty();

            for state in &dispatcher_state.states {
                if state.dependency_count == 0 {
                    circular = false;
                    break;
                }
            }

            if circular {
                panic!("circular dependency detected in dispatcher");
            }
        }

        let (event_sender, event_receiver) = crossbeam_channel::unbounded();

        // Send events
        for event in self.events.drain(..) {
            event_sender.send(event).unwrap();
        }

        Dispatcher {
            systems,
            thread_pool: ThreadPoolBuilder::new()
                .thread_name(|i| format!("Ard ECS Thread {i}"))
                .num_threads(
                    self.thread_count
                        .unwrap_or_else(|| num_cpus::get().div(2).max(1)),
                )
                .build()
                .unwrap(),
            event_to_systems,
            event_receiver,
            event_sender,
        }
    }
}

impl Events {
    /// Submit an event to the dispatcher.
    #[inline]
    pub fn submit(&self, evt: impl Event + 'static) {
        self.sender
            .send(Box::new(evt))
            .expect("unable to submit event");
    }
}

/// Helper function that performs the Bron-Kerbosch algorithm.
fn bron_kerbosch(
    r: SystemSet,
    mut p: SystemSet,
    mut x: SystemSet,
    compatibility: &[SystemSet],
    out: &mut Vec<SystemSet>,
) {
    if p.not_any() && x.not_any() {
        out.push(r);
        return;
    }

    let px = p.bitor(x);
    let pivot = px.first_one().unwrap();

    let mut nh_pivot = compatibility[pivot];
    nh_pivot.set(pivot, false);

    let p_removing_nh_pivot = p & (nh_pivot.not());

    for v in p_removing_nh_pivot.iter_ones() {
        let mut nh_v = compatibility[v];
        nh_v.set(v, false);

        let mut new_r = r;
        new_r.set(v, true);
        let new_p = p.bitand(nh_v);
        let new_x = x.bitand(nh_v);

        bron_kerbosch(new_r, new_p, new_x, compatibility, out);

        p.set(v, false);
        x.set(v, true);
    }
}
