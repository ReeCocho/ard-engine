use async_trait::async_trait;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncReadExt;

use super::{manifest::Manifest, PackageInterface, PackageOpenError, PackageReadError};

/// A package of assets contained within a folder. Useful for development purposes.
#[derive(Clone)]
pub struct FolderPackage(Arc<FolderPackageInner>);

struct FolderPackageInner {
    /// Path to the folder.
    path: PathBuf,
    /// Assets within the folder.
    manifest: Manifest,
}

#[async_trait]
impl PackageInterface for FolderPackage {
    fn manifest(&self) -> &Manifest {
        &self.0.manifest
    }

    async fn read(&self, file: &Path) -> Result<Vec<u8>, PackageReadError> {
        let meta = match self.0.manifest.assets.get(file) {
            Some(meta) => meta,
            None => return Err(PackageReadError::DoesNotExist),
        };

        let mut path = self.0.path.clone();
        path.extend(file);

        let mut contents = Vec::with_capacity(meta.uncompressed_size);
        let mut file = tokio::fs::File::open(&path).await?;
        file.read_to_end(&mut contents).await?;

        Ok(contents)
    }

    async fn read_str(&self, file: &Path) -> Result<String, PackageReadError> {
        let meta = match self.0.manifest.assets.get(file) {
            Some(meta) => meta,
            None => return Err(PackageReadError::DoesNotExist),
        };

        let mut path = self.0.path.clone();
        path.extend(file);

        let mut contents = String::with_capacity(meta.uncompressed_size);
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

        let manifest = Manifest::from_folder(path);

        Ok(FolderPackage(Arc::new(FolderPackageInner {
            path: path.into(),
            manifest,
        })))
    }
}
