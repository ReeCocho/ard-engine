use std::{
    ops::Div,
    time::{Duration, Instant},
};

use ard_ecs::{prelude::*, resource::res::Res, system::commands::Commands};
use serde::{Deserialize, Serialize};

use crate::{
    prelude::{App, AppBuilder, Destroyer, Plugin},
    stat::DirtyStatic,
};

/// Propogated once at first dispatch when using `ArdCore`.
#[derive(Debug, Default, Event, Copy, Clone)]
pub struct Start;

/// Signals to `ArdCore` that ticks should stop being propogated.
///
/// # Note
/// This is NOT the last event to be sent. If you want to handle the last event of the engine
/// then handle the `Stopping` event.
#[derive(Debug, Event, Copy, Clone)]
pub struct Stop;

/// Signal from `ArdCore` that the engine is stopping. When using `ArdCore`, this is the last
/// event to be submitted.
#[derive(Debug, Event, Copy, Clone)]
pub struct Stopping;

/// Signals one iteration of the game loop. The duration should be how much time has elapsed since
/// the last tick.
#[derive(Debug, Default, Event, Copy, Clone)]
pub struct Tick(pub Duration);

/// Current state of the core.
#[derive(Debug, Resource)]
pub struct ArdCoreState {
    stopping: bool,
}

/// A tag indicating that a particular entity is not enabled. It is up to systems to query for
/// this tag and turn off functionality when it exists. Assume that all entities without this
/// tag are enabled.
#[derive(Debug, Tag, Copy, Clone, Serialize, Deserialize)]
#[storage(CommonStorage)]
pub struct Disabled;

/// A name for an entity.
#[derive(Debug, Component, Clone, Serialize, Deserialize)]
pub struct Name(pub String);

/// The base engine plugin.
///
/// A default runner is used which, every iteration of dispatch, generates new `Tick` events until
/// the `Stop` event is received.
pub struct ArdCorePlugin;

#[derive(SystemState)]
pub struct ArdCore;

impl Default for ArdCore {
    fn default() -> Self {
        ArdCore
    }
}

impl ArdCoreState {
    #[inline]
    pub fn stopping(&self) -> bool {
        self.stopping
    }
}

impl ArdCore {
    pub fn stop(
        &mut self,
        _: Stop,
        commands: Commands,
        _: Queries<()>,
        res: Res<(Write<ArdCoreState>,)>,
    ) {
        let mut core_state = res.get_mut::<ArdCoreState>().unwrap();

        core_state.stopping = true;
        commands.events.submit(Stopping);
    }
}

#[allow(clippy::from_over_into)]
impl Into<System> for ArdCore {
    fn into(self) -> System {
        SystemBuilder::new(self).with_handler(ArdCore::stop).build()
    }
}

impl Plugin for ArdCorePlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        // Use half the number of threads on the system so we don't pin the CPU to 100%
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get().div(2).max(1))
            .build_global()
            .unwrap();

        app.add_system(ArdCore::default());
        app.add_system(Destroyer::default());
        app.add_resource(ArdCoreState { stopping: false });
        app.add_resource(DirtyStatic::default());
        app.add_event(Start);
        app.with_runner(default_core_runner);
    }
}

fn default_core_runner(mut app: App) {
    app.run_startups();

    let mut dispatcher = std::mem::take(&mut app.dispatcher).build();

    let mut last = Instant::now();
    while !app.resources.get::<ArdCoreState>().unwrap().stopping {
        // Submit tick event
        let now = Instant::now();
        dispatcher.submit(Tick(now.duration_since(last)));
        last = now;

        // Dispatch
        dispatcher.run(&mut app.world, &app.resources);
    }

    // Handle `Stopping` event
    dispatcher.run(&mut app.world, &app.resources);
}
