use crate::prelude::AssetLoader;
use std::path::{Path, PathBuf};

pub type AssetName = Path;

pub type AssetNameBuf = PathBuf;

/// Marks a type as being an asset.
pub trait Asset: Send {
    /// File extension of the asset. Must be unique amongst all other registered asset types.
    const EXTENSION: &'static str;

    /// Loader used for this asset type.
    type Loader: AssetLoader + 'static;

    /// GUI interface for modifying this asset.
    #[cfg(feature = "editor")]
    fn gui(&mut self, ui: &imgui::Ui, assets: &crate::manager::Assets) {}
}
