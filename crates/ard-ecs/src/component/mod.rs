pub mod access;
pub mod filter;
pub mod pack;

use std::any::{Any, TypeId};

pub use ard_ecs_derive::Component;

use crate::archetype::storage::{AnyArchetypeStorage, ArchetypeStorage};

/// A component represents a unique piece of data in an ECS. Components are associated with a
/// particular entity within a world.
pub trait Component: Send + Sync {
    const NAME: &'static str;
}

pub trait ComponentExt: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn into_any(self: Box<Self>) -> Box<dyn Any>;

    /// Creates the archetype storage for this component type.
    fn create_storage(&self) -> Box<dyn AnyArchetypeStorage>;
}

impl<T: Component + 'static> ComponentExt for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn create_storage(&self) -> Box<dyn AnyArchetypeStorage> {
        Box::new(ArchetypeStorage::<T>::new())
    }
}
