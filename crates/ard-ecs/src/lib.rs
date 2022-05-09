pub mod access;
pub mod archetype;
pub mod component;
pub mod dispatcher;
pub mod entity;
pub mod event;
pub mod id_map;
pub mod key;
pub mod prw_lock;
pub mod resource;
pub mod system;
pub mod tag;
pub mod world;

#[cfg(test)]
mod tests;

pub mod prelude {
    pub use crate::{
        access::*,
        component::*,
        dispatcher::*,
        entity::*,
        event::*,
        resource::{filter::*, *},
        system::{data::*, handler::*, query::*, *},
        tag::{storage::*, *},
        world::{entities::*, *},
    };
}

/// Maximum number of types that can be held in a bitset.
pub const MAX_BITSET_COUNT: usize = 128;
