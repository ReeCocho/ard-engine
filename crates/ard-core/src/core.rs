use std::{
    ops::Div,
    time::{Duration, Instant},
};

use ard_ecs::{prelude::*, resource::res::Res, system::commands::Commands};

use crate::prelude::{App, AppBuilder, Plugin};

const DEFAULT_FIXED_TICK_RATE: Duration = Duration::from_millis(33);

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

/// Occurs every iteration after `Tick`.
#[derive(Debug, Default, Event, Copy, Clone)]
pub struct PostTick(pub Duration);

/// Propogated at a fixed rate set during `ArdCore` creation. The default value is
/// `DEFAULT_FIXED_TICK_RATE`. The duration is the fixed time between propogations.
///
/// # Dispatch Slower than Fixed Rate
/// If the dispatcher runs slower than the fixed time, additionally propogations will not be sent.
/// For example, if your dispatcher is iterating half as fast as the fixed rate, only one event
/// will be sent per iteration; NOT two.
#[derive(Debug, Default, Event, Copy, Clone)]
pub struct FixedTick(pub Duration);

/// Current state of the core.
#[derive(Debug, Resource)]
pub struct ArdCoreState {
    stopping: bool,
}

/// A tag indicating that a particular entity is not enabled. It is up to systems to query for
/// this tag and turn off functionality when it exists. Assume that all entities without this
/// tag are enabled.
#[derive(Debug, Tag, Copy, Clone)]
#[storage(CommonStorage)]
pub struct Disabled;

/// A component indicating that a particular entity is static. The definition of "static" is
/// dependent on how a system uses an entity. For example, for rendering and physics it might
/// mean that an entity does not move, which can allow for better optimizations. Assume that all
/// entities without this component are "dynmaic".
#[derive(Debug, Component, Copy, Clone)]
pub struct Static;

/// The base engine plugin.
///
/// A default runner is used which, every iteration of dispatch, generates new `Tick` and
/// `PostTick` events until the `Stop` event is received. Additionally, a fixed rate
/// `FixedTick` event is propogated. The order of propogation is as follows:
///
/// `Tick` → `PostTick` → `FixedTick`
pub struct ArdCorePlugin;

#[derive(SystemState)]
pub struct ArdCore {
    fixed_rate: Duration,
    fixed_timer: Duration,
}

impl Default for ArdCore {
    fn default() -> Self {
        ArdCore {
            fixed_rate: DEFAULT_FIXED_TICK_RATE,
            fixed_timer: Duration::ZERO,
        }
    }
}

impl ArdCoreState {
    #[inline]
    pub fn stopping(&self) -> bool {
        self.stopping
    }
}

impl ArdCore {
    pub fn new(fixed_rate: Duration) -> Self {
        ArdCore {
            fixed_rate,
            fixed_timer: Duration::ZERO,
        }
    }

    pub fn tick(
        &mut self,
        tick: Tick,
        commands: Commands,
        _: Queries<()>,
        res: Res<(Write<ArdCoreState>,)>,
    ) {
        let core_state = res.get_mut::<ArdCoreState>().unwrap();

        let duration = tick.0;

        if !core_state.stopping {
            // Post tick
            commands.events.submit(PostTick(duration));

            // Check for fixed tick
            self.fixed_timer += duration;
            if self.fixed_timer >= self.fixed_rate {
                self.fixed_timer = Duration::ZERO;
                commands.events.submit(FixedTick(self.fixed_rate));
            }
        }
    }

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
        SystemBuilder::new(self)
            .with_handler(ArdCore::tick)
            .with_handler(ArdCore::stop)
            .build()
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
        app.add_resource(ArdCoreState { stopping: false });
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
