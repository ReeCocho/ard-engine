use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// A list of all the files within a package.
pub struct Manifest {
    pub assets: HashMap<PathBuf, FileMetaData>,
}

/// Meta-data describing an asset within a package.
pub struct FileMetaData {
    /// Compressed size of the file in bytes.
    pub compressed_size: usize,
    /// Uncompressed size of the file in bytes.
    pub uncompressed_size: usize,
}

impl Manifest {
    /// Recursively traverses a folder and constructs an asset manifest from the files within. This
    /// ignores symbolic links.
    pub fn from_folder(path: &Path) -> Manifest {
        let mut manifest = Manifest {
            assets: HashMap::default(),
        };

        Manifest::from_folder_recurse(path, path, &mut manifest);

        manifest
    }

    fn from_folder_recurse(root: &Path, path: &Path, manifest: &mut Manifest) {
        let iter = match path.read_dir() {
            Ok(iter) => iter,
            Err(_) => return,
        };

        for entry in iter {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            if metadata.is_symlink() {
                continue;
            } else if metadata.is_dir() {
                Manifest::from_folder_recurse(root, &entry.path(), manifest);
            } else if metadata.is_file() {
                let file_name: PathBuf = entry.path().strip_prefix(root).unwrap().into();
                manifest.assets.insert(
                    file_name,
                    FileMetaData {
                        compressed_size: metadata.len() as usize,
                        uncompressed_size: metadata.len() as usize,
                    },
                );
            }
        }
    }
}
