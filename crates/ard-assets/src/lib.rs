use prelude::*;

pub mod package;
pub mod asset;
pub mod loader;
pub mod filesystem;

pub mod prelude {
    pub use crate::package::*;
    pub use crate::asset::*;
    pub use crate::loader::*;
    pub use crate::*;
}

pub struct Assets {

}

impl Assets {
    pub fn new(manifest: PackageManifest) -> Self {
        assert!(manifest.is_valid());

        Assets {}
    }
}