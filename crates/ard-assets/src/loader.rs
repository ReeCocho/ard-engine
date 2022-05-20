use std::any::Any;

use crate::{
    handle::Handle,
    prelude::{Asset, AssetName, Assets, Package, PackageReadError},
};

use async_trait::async_trait;

pub enum AssetLoadResult<T> {
    /// Asset loaded successfully.
    Ok(T),
    /// Asset loaded successfully, but needs post load initialization.
    PostLoad(T),
    /// There was an error when loading the asset.
    Err,
}

pub enum AssetPostLoadResult {
    /// Asset finished in post load.
    Ok,
    /// Asset needs another round of post load initialization.
    PostLoad,
    /// There was an error.
    Err,
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
    ) -> AssetLoadResult<Self::Asset>;

    /// Performs post load initialization on an asset if requested.
    async fn post_load(
        &self,
        assets: Assets,
        package: Package,
        asset: Handle<Self::Asset>,
    ) -> AssetPostLoadResult;
}

pub trait AnyAssetLoader: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T: AssetLoader + 'static> AnyAssetLoader for T {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl<T: Asset> From<PackageReadError> for AssetLoadResult<T> {
    fn from(_: PackageReadError) -> Self {
        AssetLoadResult::Err
    }
}
