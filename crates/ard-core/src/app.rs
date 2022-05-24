use std::any::Any;

use ard_ecs::prelude::*;
use ard_log::LevelFilter;

use crate::prelude::Plugin;

pub struct App {
    pub resources: Resources,
    pub world: World,
    pub dispatcher: DispatcherBuilder,
    runner: fn(App),
    startup_functions: Vec<fn(&mut App)>,
}

pub struct AppBuilder {
    app: App,
}

impl Default for App {
    fn default() -> Self {
        Self {
            resources: Resources::default(),
            world: World::default(),
            dispatcher: DispatcherBuilder::new(),
            runner: |mut app| {
                app.run_startups();
                app.dispatcher.build().run(&mut app.world, &app.resources);
            },
            startup_functions: Vec::default(),
        }
    }
}

impl App {
    pub fn builder(og_filter: LevelFilter) -> AppBuilder {
        AppBuilder::new(og_filter)
    }

    /// Runs startup functions. Should be used by runners.
    pub fn run_startups(&mut self) {
        let funcs = std::mem::take(&mut self.startup_functions);
        for func in funcs {
            func(self);
        }
    }

    pub fn run(&mut self) {
        let app = std::mem::take(self);
        let runner = app.runner;
        runner(app);
    }
}

impl AppBuilder {
    pub fn new(log_filter: LevelFilter) -> Self {
        ard_log::init(log_filter);
        AppBuilder {
            app: App::default(),
        }
    }

    #[inline]
    pub fn app(&self) -> &App {
        &self.app
    }

    #[inline]
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn add_resource(&mut self, resource: impl Resource + Any) -> &mut Self {
        self.app.resources.add(resource);
        self
    }

    pub fn add_system(&mut self, system: impl Into<System>) -> &mut Self {
        self.app.dispatcher.add_system(system.into());
        self
    }

    pub fn add_event(&mut self, event: impl Event + 'static) -> &mut Self {
        self.app.dispatcher.submit(event);
        self
    }

    pub fn add_plugin(&mut self, mut plugin: impl Plugin) -> &mut Self {
        plugin.build(self);
        self
    }

    /// A startup function is called once before the first dispatch when running hte app.
    pub fn add_startup_function(&mut self, startup: fn(&mut App)) -> &mut Self {
        self.app.startup_functions.push(startup);
        self
    }

    /// A runner is the function that is used when running your app.
    pub fn with_runner(&mut self, runner: fn(App)) -> &mut Self {
        self.app.runner = runner;
        self
    }

    pub fn run(&mut self) {
        self.app.run();
    }
}
