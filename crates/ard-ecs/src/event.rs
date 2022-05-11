use std::any::Any;

pub use ard_ecs_derive::Event;

pub trait Event: Clone + Send + Sync {
    /// Name of the event, used for debugging purposes.
    const DEBUG_NAME: &'static str;
}

pub trait EventExt: Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn debug_name(&self) -> &'static str;
}

impl<E: Event + 'static> EventExt for E {
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn debug_name(&self) -> &'static str {
        E::DEBUG_NAME
    }
}
