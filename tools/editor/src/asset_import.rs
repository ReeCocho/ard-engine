use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, log::warn, render::prelude::Gui,
    window::WindowFileDropped,
};
use std::{collections::VecDeque, path::PathBuf};
use thiserror::Error;

use crate::{
    editor::EditorViewModels,
    meta::{AssetMetaData, AssetType},
    util::par_task::ParTask,
    views::assets::AssetImportView,
};

pub struct AssetImportPlugin;

#[derive(SystemState, Default)]
pub struct AssetImportSystem;

#[derive(Resource, Default)]
pub struct AssetImportState {
    pub loading: VecDeque<ParTask<PathBuf, AssetLoadError>>,
}

#[derive(Debug, Error)]
pub enum AssetLoadError {
    #[error("unknown error: {0}")]
    Error(String),
    #[error("{0:?}")]
    MoveError(fs_extra::error::ErrorKind),
    #[error("{0}")]
    IoError(std::io::Error),
    #[error("{0}")]
    BakeError(String),
    #[error("{0}")]
    MetaSaveError(ron::Error),
}

impl AssetImportSystem {
    fn file_dropped(
        &mut self,
        evt: WindowFileDropped,
        _: Commands,
        _: Queries<()>,
        res: Res<(
            Read<Assets>,
            Read<EditorViewModels>,
            Write<AssetImportState>,
        )>,
    ) {
        let assets = res.get::<Assets>().unwrap();
        let vms = res.get::<EditorViewModels>().unwrap();
        let mut state = res.get_mut::<AssetImportState>().unwrap();

        // Determine output location
        let mut dst_path = PathBuf::from("./assets/game/");
        dst_path.push(vms.assets.vm.get_folder_path());
        match evt.file.file_name() {
            Some(name) => dst_path.push(name),
            None => {
                warn!("Bad asset import attempt");
                return;
            }
        }

        let mut baked_path = PathBuf::from("./assets/baked/");
        baked_path.push(vms.assets.vm.get_folder_path());
        match evt.file.file_name() {
            Some(name) => baked_path.push(name),
            None => {
                warn!("Bad asset import attempt");
                return;
            }
        }

        self.import(evt.file, dst_path, baked_path, &mut state, &assets);
    }

    fn import(
        &self,
        src_path: PathBuf,
        dst_path: PathBuf,
        mut baked_path: PathBuf,
        state: &mut AssetImportState,
        assets: &Assets,
    ) {
        // No-op if it's a symlink
        if src_path.is_symlink() {
            warn!("Attempt to load `{src_path:?}` which is a symlink. Symlinks are not supported.");
            return;
        }

        state.loading.push_back(ParTask::new(move || {
            // Determine the asset type
            let ty = AssetType::from_ext(
                src_path
                    .extension()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or(""),
            );

            // Move the file/folder
            if src_path.is_dir() {
                fs_extra::dir::move_dir(
                    &src_path,
                    &dst_path,
                    &fs_extra::dir::CopyOptions {
                        overwrite: true,
                        skip_exist: false,
                        ..Default::default()
                    },
                )?;
            } else {
                fs_extra::file::move_file(
                    &src_path,
                    &dst_path,
                    &fs_extra::file::CopyOptions {
                        overwrite: true,
                        skip_exist: false,
                        ..Default::default()
                    },
                )?;
            }

            // If the asset is a known type, run the associated oven. Then generate the meta data.
            let meta = match ty {
                AssetType::Model => {
                    baked_path.set_extension("ard_mdl");

                    // Delete if the folder already exists
                    if baked_path.exists() {
                        fs_extra::dir::remove(&baked_path)?;
                    }

                    let res = std::process::Command::new("./tools/gltf-oven.exe")
                        .arg("--path")
                        .arg(&dst_path)
                        .arg("--out")
                        .arg(&baked_path)
                        .arg("--compress-textures")
                        .arg("--compute-tangents")
                        .output()?;

                    if let Ok(stderr) = String::from_utf8(res.stderr) {
                        if !stderr.is_empty() {
                            return Err(AssetLoadError::BakeError(stderr));
                        }
                    }

                    AssetMetaData::Model {
                        raw: dst_path.clone(),
                        baked: baked_path,
                        compress_textures: true,
                        compute_tangents: true,
                    }
                }
                _ => {
                    warn!("Loading asset `{src_path:?}` of unknown type.");
                    return Ok(PathBuf::default());
                }
            };

            // Save the meta file
            let mut meta_path = dst_path.clone();
            meta_path.set_extension("meta");
            let writer = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .open(meta_path)?;
            ron::ser::to_writer_pretty(writer, &meta, ron::ser::PrettyConfig::default())?;

            Ok(PathBuf::default())
        }));
    }
}

impl AssetImportPlugin {
    fn init(app: &mut App) {
        app.resources.add(AssetImportState::default());
        app.dispatcher.add_system(AssetImportSystem::default());

        let mut gui = app.resources.get_mut::<Gui>().unwrap();
        gui.add_view(AssetImportView);
    }
}

impl Plugin for AssetImportPlugin {
    fn name(&self) -> &str {
        "AssetImportPlugin"
    }

    fn build(&mut self, app: &mut AppBuilder) {
        app.add_startup_function(AssetImportPlugin::init);
    }
}

impl Into<System> for AssetImportSystem {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(AssetImportSystem::file_dropped)
            .build()
    }
}

impl From<fs_extra::error::Error> for AssetLoadError {
    fn from(value: fs_extra::error::Error) -> Self {
        AssetLoadError::MoveError(value.kind)
    }
}

impl From<std::io::Error> for AssetLoadError {
    fn from(value: std::io::Error) -> Self {
        AssetLoadError::IoError(value)
    }
}

impl From<ron::Error> for AssetLoadError {
    fn from(value: ron::Error) -> Self {
        AssetLoadError::MetaSaveError(value)
    }
}
