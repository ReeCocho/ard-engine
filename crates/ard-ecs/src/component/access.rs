use crate::{
    archetype::storage::access::{
        OptionalReadStorageBuffer, OptionalWriteStorageBuffer, ReadStorageBuffer,
        StorageBufferAccess, WriteStorageBuffer,
    },
    component::Component,
};

/// Represents a way to access a particular component type.
pub trait ComponentAccess {
    /// The component type being accessed.
    type Component: Component + 'static;

    /// The type of storage buffer access needed for the component access.
    type Storage: StorageBufferAccess;

    /// Indicates that the component access type is mutable.
    const MUT_ACCESS: bool;

    /// Indicates that this component access is optional.
    const IS_OPTIONAL: bool;
}

impl<'a, C: Component + 'static> ComponentAccess for &'a C {
    type Component = C;
    type Storage = ReadStorageBuffer<C>;
    const MUT_ACCESS: bool = false;
    const IS_OPTIONAL: bool = false;
}

impl<'a, C: Component + 'static> ComponentAccess for &'a mut C {
    type Component = C;
    type Storage = WriteStorageBuffer<C>;
    const MUT_ACCESS: bool = true;
    const IS_OPTIONAL: bool = false;
}

impl<'a, C: Component + 'static> ComponentAccess for Option<&'a C> {
    type Component = C;
    type Storage = OptionalReadStorageBuffer<C>;
    const MUT_ACCESS: bool = false;
    const IS_OPTIONAL: bool = true;
}

impl<'a, C: Component + 'static> ComponentAccess for Option<&'a mut C> {
    type Component = C;
    type Storage = OptionalWriteStorageBuffer<C>;
    const MUT_ACCESS: bool = true;
    const IS_OPTIONAL: bool = true;
}
