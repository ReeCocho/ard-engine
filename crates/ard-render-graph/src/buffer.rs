use std::fmt::Display;

use crate::AccessType;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(pub(crate) u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BufferUsage {
    UniformBuffer,
    /// Storage buffer used exclusively by the GPU. No CPU access.
    StorageBuffer,
    /// Storage buffer with fast read access by the CPU.
    ReadStorageBuffer,
    /// Storage buffer with fast write access by the CPU.
    WriteStorageBuffer,
    /// Buffer contents will be written to by the GPU and read by the CPU.
    ReadBack,
}

pub struct BufferDescriptor {
    pub size: u64,
    pub usage: BufferUsage,
}

#[derive(Debug, Copy, Clone)]
pub struct BufferAccessDescriptor {
    pub buffer: BufferId,
    pub access: AccessType,
}

impl Display for BufferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
