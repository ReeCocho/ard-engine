pub mod importer;
pub mod meta;
pub mod op;
pub mod mod2;

use anyhow::Result;
use path_macro::path;
use rustc_hash::FxHashMap;
use std::{ffi::OsString, path::PathBuf};

use ard_engine::ecs::prelude::*;

use crate::assets::meta::MetaFile;

pub const ASSETS_FOLDER: &'static str = "./assets/";

#[derive(Resource)]
pub struct EditorAssets {
    active_package: String,
    root: Folder,
}

pub struct Folder {
    path: PathBuf,
    active_package: String,
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
        let active_package = "main".into();
        let mut root = Folder::new("./", &active_package);
        root.inspect()?;

        Ok(Self { root, active_package })
    }

    #[inline(always)]
    pub fn active_package(&self) -> &String {
        &self.active_package
    }

    #[inline(always)]
    pub fn active_package_path(&self) -> PathBuf {
        path!(ASSETS_FOLDER / self.active_package)
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
    pub fn new(path: impl Into<PathBuf>, active_package: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            active_package: active_package.into(),
            sub_folders: FxHashMap::default(),
            assets: FxHashMap::default(),
        }
    }

    #[inline(always)]
    pub fn package_root(&self) -> PathBuf {
        path!(ASSETS_FOLDER / self.active_package)
    }

    #[inline(always)]
    pub fn abs_path(&self) -> PathBuf {
        path!(self.package_root() / self.path)
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
        let root = self.package_root();
        let folder = self.abs_path();

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
                    .or_insert_with(|| Folder::new(path, &self.active_package));
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
