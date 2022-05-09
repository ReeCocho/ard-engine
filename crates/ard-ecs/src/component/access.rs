use crate::{
    archetype::storage::access::{ReadStorageBuffer, StorageBufferAccess, WriteStorageBuffer},
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
}

impl<'a, C: Component + 'static> ComponentAccess for &'a C {
    type Component = C;
    type Storage = ReadStorageBuffer<C>;
    const MUT_ACCESS: bool = false;
}

impl<'a, C: Component + 'static> ComponentAccess for &'a mut C {
    type Component = C;
    type Storage = WriteStorageBuffer<C>;
    const MUT_ACCESS: bool = true;
}
