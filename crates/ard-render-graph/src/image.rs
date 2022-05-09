use std::fmt::Display;

use crate::{context::Context, AccessType};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub(crate) u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SizeGroupId(pub(crate) u32);

#[derive(Clone)]
pub struct ImageDescriptor<C: Context> {
    pub size_group: SizeGroupId,
    pub format: C::ImageFormat,
}

#[derive(Debug, Copy, Clone)]
pub struct ImageAccessDecriptor {
    pub image: ImageId,
    pub access: AccessType,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SizeGroup {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_layers: u32,
}

impl Display for ImageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for SizeGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
