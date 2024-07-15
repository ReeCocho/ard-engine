use camino::{Utf8Path, Utf8PathBuf};

use crate::prelude::AssetLoader;

pub type AssetName = Utf8Path;

pub type AssetNameBuf = Utf8PathBuf;

/// Marks a type as being an asset.
pub trait Asset: Send {
    /// File extension of the asset. Must be unique amongst all other registered asset types.
    const EXTENSION: &'static str;

    /// Loader used for this asset type.
    type Loader: AssetLoader + 'static;
}
