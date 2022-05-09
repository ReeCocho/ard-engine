pub trait Asset {
    /// Does this asset support hot reloading?
    const HOT_RELOAD: bool = true;

    /// File extension used by assets of this type. Must be unique from other assets.
    const EXTENSION: &'static str;
}

/// Path to a resource inside of a package. A resource can either be an asset or raw data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourcePath {
    path: String,
    ty: ResourceType,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Asset,
    Data,
}
