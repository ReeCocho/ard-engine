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
        *,
    };
}

/// Plugin to allow for asset management.
#[derive(Default)]
pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(Assets::new());
    }
}
