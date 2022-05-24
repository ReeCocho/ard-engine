use std::any::Any;
use thiserror::*;

use crate::{
    handle::Handle,
    prelude::{Asset, AssetName, Assets, Package, PackageReadError},
};

use async_trait::async_trait;

pub enum AssetLoadResult<T> {
    /// Asset loaded successfully. An additional flag is provided to indicate that this asset
    /// is persistent or not.
    Loaded { asset: T, persistent: bool },
    /// Asset loaded successfully, but needs post load initialization. An additional flag is
    /// provided to indicate that this asset is persistent or not.
    NeedsPostLoad { asset: T, persistent: bool },
}

pub enum AssetPostLoadResult {
    /// Asset finished in post load.
    Loaded,
    /// Asset needs another round of post load initialization.
    NeedsPostLoad,
}

#[derive(Debug, Error)]
pub enum AssetLoadError {
    #[error("there was an error while trying to read from a package")]
    ReadError,
    #[error("an error occured: {0}")]
    Other(Box<dyn std::error::Error>),
    #[error("an unknown error occured while loading the asset")]
    Unknown,
}

/// Used to load assets of a particular type.
#[async_trait]
pub trait AssetLoader: Send + Sync {
    /// Asset type to be loaded.
    type Asset: Asset;

    /// Load an asset from a package.
    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError>;

    /// Performs post load initialization on an asset if requested.
    async fn post_load(
        &self,
        assets: Assets,
        package: Package,
        asset: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError>;
}

pub trait AnyAssetLoader: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T: AssetLoader + 'static> AnyAssetLoader for T {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl From<PackageReadError> for AssetLoadError {
    fn from(_: PackageReadError) -> Self {
        AssetLoadError::ReadError
    }
}
