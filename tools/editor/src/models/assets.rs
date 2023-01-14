use ard_engine::assets::prelude::*;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use super::ViewModel;

pub struct AssetsViewModel {
    pub root: Folder,
    /// List of folder names in order that points to the current folder. The root is implied to be
    /// the first element.
    pub current: Vec<String>,
}

pub struct Asset {
    pub asset_name: AssetNameBuf,
    pub display_name: String,
}

#[derive(Default)]
pub struct Folder {
    pub name: String,
    pub folders: Vec<Folder>,
    pub assets: Vec<Asset>,
}

pub enum AssetsViewMessage {
    SetActiveFolder { old: Vec<String>, new: Vec<String> },
    PushFolder(String),
}

impl AssetsViewModel {
    pub fn new(assets: &Assets) -> Self {
        AssetsViewModel {
            root: Folder::new_root(assets),
            current: Vec::default(),
        }
    }

    #[inline(always)]
    pub fn get_folder_path_components(&self) -> &[String] {
        &self.current
    }

    pub fn get_folder_path(&self) -> PathBuf {
        let mut path = PathBuf::from("");
        for cur in &self.current {
            path.push(cur);
        }
        path
    }

    pub fn get_active_folder(&self) -> Option<&Folder> {
        let mut current_folder = &self.root;
        for folder_name in self.current.iter() {
            // Find the index of the sub folder
            let mut idx = None;
            for (i, folder) in current_folder.folders.iter().enumerate() {
                if folder.name == *folder_name {
                    idx = Some(i);
                    break;
                }
            }

            match idx {
                Some(idx) => {
                    current_folder = &current_folder.folders[idx];
                }
                // We didn't find the sub folder
                None => return None,
            }
        }

        Some(current_folder)
    }
}

impl ViewModel for AssetsViewModel {
    type Model<'a> = ();
    type Message = AssetsViewMessage;

    fn apply<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message {
        match &msg {
            AssetsViewMessage::SetActiveFolder { new, .. } => {
                self.current = new.clone();
            }
            AssetsViewMessage::PushFolder(folder) => {
                self.current.push(folder.clone());
            }
        }

        msg
    }

    fn undo<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message {
        match &msg {
            AssetsViewMessage::SetActiveFolder { old, .. } => {
                self.current = old.clone();
            }
            AssetsViewMessage::PushFolder(_) => {
                self.current.pop();
            }
        }

        msg
    }

    fn update<'a>(&mut self, model: &mut Self::Model<'a>) {}
}

impl Folder {
    fn new(name: String) -> Self {
        Folder {
            name,
            folders: Vec::default(),
            assets: Vec::default(),
        }
    }

    fn new_root(assets: &Assets) -> Self {
        let mut root = Folder::default();

        // The only assets we care about are the ones in the game and baked package
        let game_pak_id = assets
            .get_package_by_name(Path::new("./assets/baked/"))
            .expect("game package must exist");

        // First pass to register assets
        for pair in assets.assets() {
            let asset = pair.value();
            let name = asset.name().file_name().unwrap();

            // Skip if not a game package
            if asset.package() != game_pak_id {
                continue;
            }

            // Skip if meta file
            if let Some(ext) = asset.name().extension() {
                if ext == "meta" {
                    continue;
                }
            }

            // Skip if the file doesn't exist
            let mut abs_path = PathBuf::from("./assets/baked/");
            abs_path.push(asset.name());
            if !abs_path.exists() {
                continue;
            }

            // First, construct all the folders of the asset
            let mut current_dir = &mut root;

            'outer: for folder in asset.name().iter() {
                // Skip if root
                if folder.is_empty() || folder == "." || folder == "/" {
                    continue;
                }

                // Skip if actual asset
                if folder == name {
                    continue;
                }

                // Skip if we already have the folder
                let folder_name = folder.to_str().unwrap();
                for (i, cur_folder) in current_dir.folders.iter().enumerate() {
                    if cur_folder.name == folder_name {
                        current_dir = &mut current_dir.folders[i];
                        continue 'outer;
                    }
                }

                // Add the folder
                current_dir
                    .folders
                    .push(Folder::new(String::from(folder_name)));
                current_dir = current_dir.folders.last_mut().unwrap();
            }

            current_dir.assets.push(Asset {
                asset_name: AssetNameBuf::from(asset.name()),
                display_name: asset
                    .name()
                    .file_name()
                    .unwrap_or(OsStr::new("[INVALID NAME]"))
                    .to_str()
                    .unwrap_or("[INVALID NAME]")
                    .to_string(),
            });
        }

        // Second pass to detect empty folders
        fn find_dirs(folders: &mut Vec<Folder>, dir: &Path) {
            // Find unadded directories
            'outer: for dir in dir.read_dir().unwrap() {
                let dir = dir.unwrap();
                let meta_data = dir.metadata().unwrap();

                // Skip if not a directory
                if !meta_data.is_dir() {
                    continue;
                }

                // Skip if symlink
                if meta_data.is_symlink() {
                    continue;
                }

                let dir_name: String = dir.file_name().to_str().unwrap().into();

                // Skip if we already have this folder
                for folder in folders.iter() {
                    if folder.name == dir_name {
                        continue 'outer;
                    }
                }

                folders.push(Folder::new(dir_name));
            }

            // Recurse on all folders
            for folder in folders.iter_mut() {
                let mut dir: PathBuf = dir.into();
                dir.push(&folder.name);
                find_dirs(&mut folder.folders, &dir);
            }
        }

        // Find empty folders
        find_dirs(&mut root.folders, &Path::new("./assets/baked/"));

        root
    }
}
