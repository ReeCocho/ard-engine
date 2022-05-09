use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::prelude::Asset;

pub trait Package {
    fn new(path: &Path) -> Self;

    /// Request that an asset be loaded.
    fn request_load(&self, path: &str);
}

/// Contains all packages to be used by an asset manager.
#[derive(Debug, Default)]
pub struct PackageManifest {
    packages: Vec<PathBuf>,
}

impl PackageManifest {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn packages(&self) -> &[PathBuf] {
        &self.packages
    }

    /// Verify that the manifest is valid. This is true if there are no duplicate packages.
    pub fn is_valid(&self) -> bool {
        let mut packages = HashSet::<PathBuf>::default();
        for package in &self.packages {
            if !packages.insert(package.clone()) {
                return false;
            }
        }
        true
    }

    /// Add a new package to the manifest.
    #[inline]
    pub fn add(mut self, path: &Path) -> Self {
        self.packages.push(PathBuf::from(path));
        self
    }
}
