use ard_ecs::prelude::*;
use serde::{Deserialize, Serialize};

/// Right now, this just flags entities as being in `StaticGroup(0)`, but in the future we can use
/// it to put entities into different groups based on other things (position, for example).
#[derive(Debug, Component, Clone, Copy, Serialize, Deserialize)]
pub struct MarkStatic;
