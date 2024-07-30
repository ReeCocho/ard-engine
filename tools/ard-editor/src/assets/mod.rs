pub mod importer;
pub mod meta;
pub mod op;
// pub mod op2;

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::Result;
use ard_engine::{assets::prelude::*, ecs::prelude::*};
use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use meta::MetaFile;
use path_macro::path;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

pub const ASSETS_FOLDER: &'static str = "./assets/";

#[derive(Resource, Default)]
pub struct CurrentAssetPath(Utf8PathBuf);

#[derive(Resource)]
pub struct EditorAssets {
    active_package: PackageId,
    active_package_root: PathBuf,
    active_assets_root: Utf8PathBuf,
    root: Folder,
}

#[derive(Serialize, Deserialize)]
pub struct EditorAssetsManifest(Folder);
pub struct AssetManifestLoader;

#[derive(Serialize, Deserialize, Clone)]
pub struct Folder {
    path: Utf8PathBuf,
    sub_folders: FxHashMap<String, Folder>,
    assets: FxHashMap<String, EditorAsset>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EditorAsset {
    meta_file: MetaFile,
    meta_path: Utf8PathBuf,
    #[serde(skip)]
    packages: BTreeSet<PackageId>,
}

impl CurrentAssetPath {
    #[inline(always)]
    pub fn path(&self) -> &Utf8Path {
        &self.0
    }

    #[inline(always)]
    pub fn path_mut(&mut self) -> &mut Utf8PathBuf {
        &mut self.0
    }
}

impl EditorAssets {
    pub fn new(assets: &Assets) -> Result<Self> {
        let packages = assets.packages();

        const ACTIVE_NAME: &'static str = "main";

        let mut active_package = None;
        let mut root = Folder {
            path: Utf8PathBuf::default(),
            sub_folders: FxHashMap::default(),
            assets: FxHashMap::default(),
        };

        packages.iter().enumerate().for_each(|(i, package)| {
            let id = PackageId::from(i);

            if package.path().file_name() == Some(OsStr::new(ACTIVE_NAME)) {
                active_package = Some(id);
            }

            // Load manifest from LOF files
            if package.path().extension() == Some(OsStr::new("lof")) {
                let mut manifest_name =
                    AssetNameBuf::from(package.path().file_stem().unwrap().to_str().unwrap());
                manifest_name.set_extension("manifest");

                let handle = assets.load::<EditorAssetsManifest>(&manifest_name).unwrap();
                assets.wait_for_load(&handle);

                let mut manifest = assets.get_mut(&handle).unwrap();
                manifest.0.set_package(id);

                root.merge_from(&manifest.0);
                return;
            }

            // Parse paths of everything else
            let assets_path = Utf8PathBuf::from_path_buf(path!(
                ASSETS_FOLDER / package.path().file_name().unwrap()
            ))
            .unwrap();
            let package_folder = Folder::from_folder(assets_path, id).unwrap();
            root.merge_from(&package_folder);
        });

        let active_package = active_package.unwrap();
        let active_package_root = packages[usize::from(active_package)].path().to_owned();
        let active_assets_root = Utf8PathBuf::from_path_buf(path!(
            ASSETS_FOLDER
                / packages[usize::from(active_package)]
                    .path()
                    .file_name()
                    .unwrap()
        ))
        .unwrap();

        Ok(Self {
            active_package,
            active_package_root,
            active_assets_root,
            root,
        })
    }

    pub fn build_manifest(&self) -> EditorAssetsManifest {
        EditorAssetsManifest(self.root.clone())
    }

    #[inline(always)]
    pub fn active_package_root(&self) -> &Path {
        &self.active_package_root
    }

    #[inline(always)]
    pub fn active_assets_root(&self) -> &Utf8Path {
        &self.active_assets_root
    }

    #[inline(always)]
    pub fn active_package_id(&self) -> PackageId {
        self.active_package
    }

    pub fn find_folder(&self, path: impl AsRef<Utf8Path>) -> Option<&Folder> {
        let mut root = &self.root;
        for component in path.as_ref().iter() {
            root = root.sub_folders.get(component)?;
        }
        Some(root)
    }

    pub fn find_asset(&self, path: impl AsRef<Utf8Path>) -> Option<&EditorAsset> {
        let path: &Utf8Path = path.as_ref();
        let mut root = &self.root;
        let mut iter = path.iter().peekable();

        while let Some(component) = iter.next() {
            if iter.peek().is_none() {
                return root.assets.get(component);
            } else {
                root = root.sub_folders.get(component)?;
            }
        }

        None
    }

    pub fn find_asset_mut(&mut self, path: impl AsRef<Utf8Path>) -> Option<&mut EditorAsset> {
        let path: &Utf8Path = path.as_ref();
        let mut root = &mut self.root;
        let mut iter = path.iter().peekable();

        while let Some(component) = iter.next() {
            if iter.peek().is_none() {
                return root.assets.get_mut(component);
            } else {
                root = root.sub_folders.get_mut(component)?;
            }
        }

        None
    }

    pub fn scan_for(&mut self, path: impl AsRef<Utf8Path>) -> Result<()> {
        let path: &Utf8Path = path.as_ref();
        let rel_path = path.strip_prefix(&self.active_assets_root)?;

        // TODO: Too much nesting
        let mut folder = &mut self.root;
        let mut iter = rel_path.iter().peekable();
        while let Some(component) = iter.next() {
            if iter.peek().is_none() {
                if path.is_dir() {
                    // Check if folder already exists
                    if folder.sub_folders.contains_key(component) {
                        return Ok(());
                    }

                    let new_folder = Folder::from_folder_rescurse(
                        self.active_assets_root.as_path().as_std_path(),
                        path.as_std_path(),
                        self.active_package,
                    )?;
                    folder.sub_folders.insert(component.to_owned(), new_folder);
                } else if path.is_file() && path.extension() == Some(MetaFile::EXTENSION) {
                    match folder.assets.get_mut(component) {
                        Some(asset) => {
                            // Check if asset already exists in the current package
                            if asset.package() == self.active_package {
                                return Ok(());
                            }

                            // Only update if we would shadow the asset
                            asset.packages.insert(self.active_package);
                        }
                        None => {
                            let f = std::fs::File::open(path)?;
                            let reader = std::io::BufReader::new(f);
                            let meta_file = ron::de::from_reader::<_, MetaFile>(reader)?;

                            folder.assets.insert(
                                component.to_owned(),
                                EditorAsset {
                                    meta_file,
                                    meta_path: rel_path.to_owned(),
                                    packages: BTreeSet::from([self.active_package]),
                                },
                            );
                        }
                    }
                }
            } else {
                folder = match folder.sub_folders.get_mut(component) {
                    Some(folder) => folder,
                    None => {
                        return Err(anyhow::Error::msg("path does not exist in active package"))
                    }
                }
            }
        }

        Ok(())
    }

    pub fn remove_from_active_package(&mut self, path: impl AsRef<Utf8Path>) {
        let path: &Utf8Path = path.as_ref();
        let mut root = &mut self.root;
        let mut iter = path.iter().peekable();

        while let Some(component) = iter.next() {
            if iter.peek().is_none() {
                let name = path.file_name().unwrap_or("");

                if path.extension().is_none() {
                    let folder = match root.sub_folders.get(name) {
                        Some(folder) => folder,
                        None => return,
                    };

                    if !folder.contained_only_within(self.active_package) {
                        return;
                    }

                    root.sub_folders.remove(name);
                } else {
                    let asset = match root.assets.get_mut(name) {
                        Some(asset) => asset,
                        None => return,
                    };

                    asset.packages.remove(&self.active_package);
                    if asset.packages.is_empty() {
                        root.assets.remove(name);
                    }
                }
            } else {
                root = match root.sub_folders.get_mut(component) {
                    Some(root) => root,
                    None => return,
                };
            }
        }
    }
}

impl Folder {
    pub fn from_folder(root: impl Into<PathBuf>, package: PackageId) -> Result<Self> {
        let root: PathBuf = root.into();
        assert!(root.is_dir(), "attempt to load file as folder");
        Self::from_folder_rescurse(&root, &root, package)
    }

    fn from_folder_rescurse(root: &Path, cur_dir: &Path, package: PackageId) -> Result<Self> {
        assert!(cur_dir.is_dir(), "attempt to load file as folder");

        let mut folder = Folder {
            path: Utf8PathBuf::try_from(cur_dir.strip_prefix(root)?.to_owned())?,
            sub_folders: FxHashMap::default(),
            assets: FxHashMap::default(),
        };

        for entry in cur_dir.read_dir()? {
            let entry = entry?;
            let name = Utf8PathBuf::try_from(PathBuf::from(entry.file_name()))?;
            let file_type = entry.file_type()?;

            if file_type.is_symlink() {
                continue;
            } else if file_type.is_dir() {
                let new_dir = entry.path();
                folder.sub_folders.insert(
                    name.as_str().to_owned(),
                    Self::from_folder_rescurse(root, &new_dir, package)?,
                );
            } else if file_type.is_file() && name.extension() == Some(MetaFile::EXTENSION) {
                let new_file = entry.path();
                let f = std::fs::File::open(&new_file)?;
                let reader = std::io::BufReader::new(f);
                let meta_file = ron::de::from_reader::<_, MetaFile>(reader)?;

                let asset = EditorAsset {
                    meta_file,
                    meta_path: {
                        let mut meta_path = folder.path.clone();
                        meta_path.push(&name);
                        meta_path
                    },
                    packages: BTreeSet::from([package]),
                };

                folder.assets.insert(name.as_str().to_owned(), asset);
            }
        }

        Ok(folder)
    }

    #[inline(always)]
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    #[inline(always)]
    pub fn sub_folders(&self) -> &FxHashMap<String, Folder> {
        &self.sub_folders
    }

    #[inline(always)]
    pub fn assets(&self) -> &FxHashMap<String, EditorAsset> {
        &self.assets
    }

    /// Takes the `src` folder and merges into `self`.
    pub fn merge_from(&mut self, src: &Folder) {
        src.assets.iter().for_each(|(meta_file, asset)| {
            let entry = self
                .assets
                .entry(meta_file.clone())
                .or_insert_with(|| EditorAsset {
                    meta_file: asset.meta_file.clone(),
                    meta_path: asset.meta_path.clone(),
                    packages: BTreeSet::default(),
                });

            entry.packages.insert(asset.package());
        });

        src.sub_folders.iter().for_each(|(folder_name, folder)| {
            self.sub_folders
                .entry(folder_name.clone())
                .or_insert_with(|| Folder {
                    path: folder.path.clone(),
                    sub_folders: FxHashMap::default(),
                    assets: FxHashMap::default(),
                })
                .merge_from(folder);
        });
    }

    pub fn contained_only_within(&self, package: PackageId) -> bool {
        for asset in self.assets.values() {
            if asset.packages.len() > 1 || !asset.packages.contains(&package) {
                return false;
            }
        }

        for sub_folder in self.sub_folders.values() {
            if !sub_folder.contained_only_within(package) {
                return false;
            }
        }

        return true;
    }

    fn set_package(&mut self, package: PackageId) {
        self.assets.values_mut().for_each(|asset| {
            asset.packages = BTreeSet::from([package]);
        });

        self.sub_folders.values_mut().for_each(|sub_folder| {
            sub_folder.set_package(package);
        });
    }
}

impl EditorAsset {
    #[inline(always)]
    pub fn meta_file(&self) -> &MetaFile {
        &self.meta_file
    }

    #[inline(always)]
    pub fn meta_path(&self) -> &Utf8Path {
        &self.meta_path
    }

    #[inline(always)]
    pub fn raw_path(&self) -> Utf8PathBuf {
        self.meta_path.with_extension("")
    }

    #[inline(always)]
    pub fn is_shadowing(&self) -> bool {
        self.packages.len() > 1
    }

    #[inline(always)]
    pub fn package(&self) -> PackageId {
        *self.packages.last().unwrap()
    }
}

impl Asset for EditorAssetsManifest {
    const EXTENSION: &'static str = "manifest";
    type Loader = AssetManifestLoader;
}

#[async_trait]
impl AssetLoader for AssetManifestLoader {
    type Asset = EditorAssetsManifest;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        let raw = package.read(asset.to_owned()).await?;
        let manifest = match bincode::deserialize(&raw) {
            Ok(manifest) => manifest,
            Err(_) => return Err(AssetLoadError::Other("unable to parse manifest".into())),
        };

        Ok(AssetLoadResult::Loaded {
            asset: manifest,
            persistent: true,
        })
    }

    async fn post_load(
        &self,
        _assets: Assets,
        _package: Package,
        _handle: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        Ok(AssetPostLoadResult::Loaded)
    }
}
