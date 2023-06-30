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
    pub use crate::access::Read;
    pub use crate::access::Write;
    pub use crate::component::pack::ComponentPack;
    pub use crate::component::Component;
    pub use crate::component::ComponentExt;
    pub use crate::dispatcher::Dispatcher;
    pub use crate::dispatcher::DispatcherBuilder;
    pub use crate::entity::Entity;
    pub use crate::event::Event;
    pub use crate::event::EventExt;
    pub use crate::resource::res::Res;
    pub use crate::resource::Resource;
    pub use crate::resource::Resources;
    pub use crate::system::commands::Commands;
    pub use crate::system::data::Everything;
    pub use crate::system::handler::EventHandler;
    pub use crate::system::query::ComponentQuery;
    pub use crate::system::query::EntityComponentQuery;
    pub use crate::system::query::EntityComponentTagQuery;
    pub use crate::system::query::Queries;
    pub use crate::system::query::Query;
    pub use crate::system::query::QueryFilter;
    pub use crate::system::query::SingleComponentQuery;
    pub use crate::system::query::SingleComponentTagQuery;
    pub use crate::system::query::SingleQuery;
    pub use crate::system::query::SingleTagQuery;
    pub use crate::system::System;
    pub use crate::system::SystemBuilder;
    pub use crate::system::SystemState;
    pub use crate::system::SystemStateExt;
    pub use crate::tag::storage::CommonStorage;
    pub use crate::tag::storage::UncommonStorage;
    pub use crate::tag::Tag;
    pub use crate::tag::TagExt;
    pub use crate::tag::Tags;
    pub use crate::world::entities::Entities;
    pub use crate::world::entities::EntityCommands;
    pub use crate::world::World;
    pub use ard_ecs_derive::SystemState;
    pub use ard_ecs_derive::*;
}

/// Maximum number of types that can be held in a bitset.
pub const MAX_BITSET_COUNT: usize = 128;
