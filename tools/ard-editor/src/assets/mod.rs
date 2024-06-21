pub mod importer;
pub mod meta;

use anyhow::Result;
use path_macro::path;
use rustc_hash::FxHashMap;
use std::{ffi::OsString, path::PathBuf};

use ard_engine::ecs::prelude::*;

use crate::assets::meta::MetaFile;

pub const ASSETS_FOLDER: &'static str = "./assets/";

#[derive(Resource)]
pub struct EditorAssets {
    root: Folder,
}

pub struct Folder {
    path: PathBuf,
    sub_folders: FxHashMap<OsString, Folder>,
    assets: FxHashMap<OsString, FolderAsset>,
}

#[derive(Clone)]
pub struct FolderAsset {
    pub meta: MetaFile,
    pub meta_path: PathBuf,
    pub raw_path: PathBuf,
}

impl EditorAssets {
    pub fn new() -> Result<Self> {
        let mut root = Folder::new("./");
        root.inspect()?;

        Ok(Self { root })
    }

    #[inline(always)]
    pub fn root(&self) -> &Folder {
        &self.root
    }

    pub fn find_folder_mut(&mut self, path: impl Into<PathBuf>) -> Option<&mut Folder> {
        let path: PathBuf = path.into();
        let mut folder = &mut self.root;
        for component in path.iter() {
            folder = folder.sub_folders.get_mut(component)?;
        }
        Some(folder)
    }

    pub fn add_meta_file(&mut self, path: impl Into<PathBuf>) {
        let path: PathBuf = path.into();
        let parent = path.parent().map(|p| p.to_owned()).unwrap_or_default();
        self.find_folder_mut(parent).map(|f| f.inspect());
    }
}

impl Folder {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            sub_folders: FxHashMap::default(),
            assets: FxHashMap::default(),
        }
    }

    #[inline(always)]
    pub fn abs_path(&self) -> PathBuf {
        path!(ASSETS_FOLDER / "main" / self.path)
    }

    #[inline(always)]
    pub fn sub_folders(&self) -> &FxHashMap<OsString, Folder> {
        &self.sub_folders
    }

    #[inline(always)]
    pub fn assets(&self) -> &FxHashMap<OsString, FolderAsset> {
        &self.assets
    }

    pub fn inspect(&mut self) -> Result<()> {
        let root = path!(ASSETS_FOLDER / "main");
        let folder = path!(root / self.path);

        self.sub_folders.clear();
        self.assets.clear();

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
                let path = path.strip_prefix(&root).unwrap();
                self.sub_folders
                    .entry(name)
                    .or_insert_with(|| Folder::new(path));
            }
            // Read in the meta file
            else if file_ty.is_file()
                && path.extension() == Some(std::ffi::OsStr::new(MetaFile::EXTENSION))
            {
                let f = std::fs::File::open(&path)?;
                let reader = std::io::BufReader::new(f);
                let meta_data = ron::de::from_reader::<_, MetaFile>(reader)?;
                self.assets.insert(
                    name,
                    FolderAsset {
                        meta: meta_data,
                        raw_path: {
                            let mut path = path.clone();
                            path.set_extension("");
                            path
                        },
                        meta_path: path,
                    },
                );
            }
        }

        Ok(())
    }
}
