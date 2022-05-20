pub mod asset;
pub mod handle;
pub mod loader;
pub mod manager;
pub mod package;

#[cfg(test)]
mod tests;

use ard_core::prelude::*;
use prelude::Assets;

pub mod prelude {
    pub use crate::{
        asset::*,
        handle::*,
        loader::*,
        manager::*,
        package::*,
        package::{folder::*, manifest::*},
    };
}

/// Plugin to allow for asset management.
pub struct AssetsPlugin {
    /// Number of threads to use for loading assets. Must be non-zero.
    pub thread_count: usize,
}

impl Default for AssetsPlugin {
    fn default() -> Self {
        Self { thread_count: 2 }
    }
}

impl Plugin for AssetsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        assert_ne!(self.thread_count, 0);
        app.add_resource(Assets::new(self.thread_count));
    }
}
