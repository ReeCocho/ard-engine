use crate::prelude::{System, SystemDataAccesses};

#[derive(Default)]
pub struct Stage {
    /// System that must run on the main thread/requested unique access to entities.
    pub(crate) main: Option<System>,
    /// Systems that may run in parallel with each other.
    pub(crate) parallel: Vec<System>,
    accesses: SystemDataAccesses,
}

impl Stage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the optional main system for this stage.
    #[inline]
    pub fn main(&self) -> &Option<System> {
        &self.main
    }

    /// Get the parallel systems in this stage.
    #[inline]
    pub fn parallel(&self) -> &[System] {
        &self.parallel
    }

    #[inline]
    pub fn can_hold(&self, system: &System) -> bool {
        system.state.accesses().compatible_with(&self.accesses)
            && (!system.exclusive || !system.entities || self.main.is_none())
    }

    /// Adds a new system to the stage. If the stage couldn't hold the system, it is rturned.
    pub fn add_system(&mut self, system: impl Into<System>) -> Option<System> {
        let system: System = system.into();
        let accesses = system.state.accesses();

        // Ensure that the stage can hold the new system
        if self.can_hold(&system) {
            self.accesses += accesses;

            if system.exclusive || system.entities {
                self.main = Some(system);
            } else {
                self.parallel.push(system);
            }

            None
        } else {
            Some(system)
        }
    }
}
