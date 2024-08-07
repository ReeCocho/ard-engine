pub mod folder;
pub mod lof;
pub mod manifest;

use std::path::Path;

use async_trait::async_trait;
use camino::Utf8PathBuf;
use crossbeam_utils::sync::{ShardedLockReadGuard, ShardedLockWriteGuard};
use enum_dispatch::enum_dispatch;
use lof::LofPackage;
use thiserror::Error;

use crate::prelude::AssetName;

use self::{folder::FolderPackage, manifest::Manifest};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId(usize);

/// A package contains a list of assets for the asset manager to load.
#[enum_dispatch]
#[derive(Clone)]
pub enum Package {
    Folder(FolderPackage),
    Lof(LofPackage),
}

#[derive(Debug, Error)]
pub enum PackageOpenError {
    #[error("the package at the given path does not exist")]
    DoesNotExist,
    #[error("invalid permissions to read the file")]
    InvalidPermissions,
    #[error("an unknown error occured")]
    Unknown,
}

#[derive(Debug, Error)]
pub enum PackageReadError {
    #[error("the asset ({0}) at the given path within the package does not exist")]
    DoesNotExist(Utf8PathBuf),
    #[error("an unknown error occured")]
    Unknown,
}

/// Used to load assets from disk.
#[async_trait]
#[enum_dispatch(Package)]
pub trait PackageInterface: Clone + Send {
    /// Path to the package.
    fn path(&self) -> &Path;

    /// Attempt to add a register an asset with the package. If the asset exists and was added,
    /// 'true' is returned.
    fn register_asset(&self, name: &AssetName) -> bool;

    /// Retrieve a manifest of all assets within the package.
    fn manifest(&self) -> ShardedLockReadGuard<Manifest>;

    /// Retrieve a manifest of all assets within the package mutably.
    fn manifest_mut(&self) -> ShardedLockWriteGuard<Manifest>;

    /// Reads the contents of a file within the package and returns the bytes.
    async fn read(&self, file: Utf8PathBuf) -> Result<Vec<u8>, PackageReadError>;

    /// Reads the contents of a file within the package and returns the bytes as a string.
    async fn read_str(&self, file: Utf8PathBuf) -> Result<String, PackageReadError>;
}

impl From<std::io::Error> for PackageReadError {
    fn from(_: std::io::Error) -> Self {
        PackageReadError::Unknown
    }
}

impl Default for PackageId {
    fn default() -> Self {
        Self(usize::MAX)
    }
}

impl From<usize> for PackageId {
    #[inline(always)]
    fn from(id: usize) -> Self {
        PackageId(id)
    }
}

impl From<PackageId> for usize {
    #[inline(always)]
    fn from(id: PackageId) -> Self {
        id.0
    }
}
