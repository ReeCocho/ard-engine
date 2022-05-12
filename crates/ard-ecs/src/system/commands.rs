use crate::{dispatcher::Events, prelude::EntityCommands};

/// Used to allow a system to communicate with the world and dispatcher.
pub struct Commands {
    pub entities: EntityCommands,
    pub events: Events,
}
