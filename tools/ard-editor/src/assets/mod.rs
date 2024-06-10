pub mod importer;
pub mod meta;

use anyhow::Result;
use rustc_hash::FxHashMap;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use ard_engine::ecs::prelude::*;

use crate::assets::meta::MetaFile;

#[derive(Resource)]
pub struct EditorAssets {
    root: Folder,
}

#[derive(Default)]
pub struct Folder {
    sub_folders: FxHashMap<OsString, Folder>,
    assets: FxHashMap<OsString, MetaFile>,
}

impl EditorAssets {
    pub fn new(packages_folder: impl Into<PathBuf>) -> Result<Self> {
        let packages_folder: PathBuf = packages_folder.into();

        let mut root = Folder::default();
        for entry in packages_folder.read_dir()? {
            let entry = entry?;
            root.inspect(entry.path())?;
        }

        Ok(Self { root })
    }

    #[inline(always)]
    pub fn root(&self) -> &Folder {
        &self.root
    }

    pub fn add_meta_file(&mut self, meta_file: MetaFile) {
        let mut cur_folder = &mut self.root;
        let path = meta_file.raw.clone();
        let mut components = path.components().skip(2).peekable();
        while let Some(component) = components.next() {
            let component = match component {
                std::path::Component::Normal(component) => component,
                _ => continue,
            };

            if components.peek().is_none() {
                cur_folder.assets.insert(component.into(), meta_file);
                break;
            } else {
                cur_folder = cur_folder.sub_folders.entry(component.into()).or_default();
            }
        }
    }
}

impl Folder {
    #[inline(always)]
    pub fn sub_folders(&self) -> &FxHashMap<OsString, Folder> {
        &self.sub_folders
    }

    #[inline(always)]
    pub fn assets(&self) -> &FxHashMap<OsString, MetaFile> {
        &self.assets
    }

    pub fn inspect(&mut self, folder: impl Into<PathBuf>) -> Result<()> {
        let root: PathBuf = folder.into();
        self.inspect_recurse(&root, root.clone())
    }

    fn inspect_recurse(&mut self, root: &Path, folder: PathBuf) -> Result<()> {
        for entry in folder.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let file_ty = entry.file_type()?;

            // Skip symlinks
            if file_ty.is_symlink() {
                continue;
            }
            // Recurse if this is another folder
            else if file_ty.is_dir() {
                let dir = self.sub_folders.entry(name).or_default();
                dir.inspect_recurse(root, path.clone())?;
            }
            // Read in the meta file
            else if file_ty.is_file()
                && path.extension() == Some(std::ffi::OsStr::new(MetaFile::EXTENSION))
            {
                let f = std::fs::File::open(path)?;
                let reader = std::io::BufReader::new(f);
                let meta_data = ron::de::from_reader::<_, MetaFile>(reader)?;
                self.assets.insert(name, meta_data);
            }
        }

        Ok(())
    }
}
