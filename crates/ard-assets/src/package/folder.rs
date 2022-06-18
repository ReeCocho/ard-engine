use async_trait::async_trait;
use crossbeam_utils::sync::{ShardedLock, ShardedLockReadGuard, ShardedLockWriteGuard};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncReadExt;

use crate::prelude::{AssetName, AssetNameBuf, FileMetaData};

use super::{manifest::Manifest, PackageInterface, PackageOpenError, PackageReadError};

/// A package of assets contained within a folder. Useful for development purposes.
#[derive(Clone)]
pub struct FolderPackage(Arc<FolderPackageInner>);

struct FolderPackageInner {
    /// Path to the folder.
    path: PathBuf,
    /// Assets within the folder.
    manifest: ShardedLock<Manifest>,
}

#[async_trait]
impl PackageInterface for FolderPackage {
    #[inline]
    fn path(&self) -> &Path {
        &self.0.path
    }

    #[inline]
    fn manifest(&self) -> ShardedLockReadGuard<Manifest> {
        self.0.manifest.read().unwrap()
    }

    #[inline]
    fn manifest_mut(&self) -> ShardedLockWriteGuard<Manifest> {
        self.0.manifest.write().unwrap()
    }

    fn register_asset(&self, name: &AssetName) -> bool {
        let mut path = self.0.path.clone();
        path.push(name);

        if path.exists() {
            let meta = match path.metadata() {
                Ok(meta) => FileMetaData {
                    compressed_size: meta.len() as usize,
                    uncompressed_size: meta.len() as usize,
                },
                Err(_) => return false,
            };

            let mut manifest = self.0.manifest.write().unwrap();
            manifest.assets.insert(AssetNameBuf::from(name), meta);

            true
        } else {
            false
        }
    }

    async fn read(&self, file: &Path) -> Result<Vec<u8>, PackageReadError> {
        let (mut contents, path) = {
            let manifest = self.0.manifest.read().unwrap();
            let meta = match manifest.assets.get(file) {
                Some(meta) => meta,
                None => return Err(PackageReadError::DoesNotExist),
            };

            let mut path = self.0.path.clone();
            path.extend(file);

            (Vec::with_capacity(meta.uncompressed_size), path)
        };

        let mut file = tokio::fs::File::open(&path).await?;
        file.read_to_end(&mut contents).await?;

        Ok(contents)
    }

    async fn read_str(&self, file: &Path) -> Result<String, PackageReadError> {
        let (mut contents, path) = {
            let manifest = self.0.manifest.read().unwrap();
            let meta = match manifest.assets.get(file) {
                Some(meta) => meta,
                None => return Err(PackageReadError::DoesNotExist),
            };

            let mut path = self.0.path.clone();
            path.extend(file);

            (String::with_capacity(meta.uncompressed_size), path)
        };

        let mut file = tokio::fs::File::open(&path).await?;
        file.read_to_string(&mut contents).await?;

        Ok(contents)
    }
}

impl FolderPackage {
    pub fn open(path: &Path) -> Result<Self, PackageOpenError> {
        if !path.exists() || !path.is_dir() {
            return Err(PackageOpenError::DoesNotExist);
        }

        let manifest = ShardedLock::new(Manifest::from_folder(path));

        Ok(FolderPackage(Arc::new(FolderPackageInner {
            path: path.into(),
            manifest,
        })))
    }
}
