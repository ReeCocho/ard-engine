use std::any::Any;

pub use ard_ecs_derive::Event;

pub trait Event: Clone + Send + Sync {}

pub trait EventExt: Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<E: Event + 'static> EventExt for E {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
