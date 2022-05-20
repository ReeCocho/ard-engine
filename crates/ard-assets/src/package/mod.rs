pub mod folder;
pub mod manifest;

use std::path::Path;

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use thiserror::Error;

use self::{folder::FolderPackage, manifest::Manifest};

/// A package contains a list of assets for the asset manager to load.
#[enum_dispatch]
#[derive(Clone)]
pub enum Package {
    Folder(FolderPackage),
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
    #[error("the asset at the given path within the package does not exist")]
    DoesNotExist,
    #[error("an unknown error occured")]
    Unknown,
}

/// Used to load assets from disk.
#[async_trait]
#[enum_dispatch(Package)]
pub trait PackageInterface: Clone + Send {
    /// Retrieve a manifest of all assets within the package.
    fn manifest(&self) -> &Manifest;

    /// Reads the contents of a file within the package and returns the bytes.
    async fn read(&self, file: &Path) -> Result<Vec<u8>, PackageReadError>;

    /// Reads the contents of a file within the package and returns the bytes as a string.
    async fn read_str(&self, file: &Path) -> Result<String, PackageReadError>;
}

impl From<std::io::Error> for PackageReadError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => PackageReadError::DoesNotExist,
            _ => PackageReadError::Unknown,
        }
    }
}
