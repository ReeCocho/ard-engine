use std::path::Path;

/// Used to read data from packages.
pub trait FileSystem {
    fn new(path: &Path) -> Self;
}