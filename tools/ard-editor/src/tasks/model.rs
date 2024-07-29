use anyhow::Result;
use ard_engine::{
    assets::{asset::AssetNameBuf, manager::Assets},
    ecs::prelude::*,
    log::*,
};
use camino::Utf8Path;
use path_macro::path;
use std::path::PathBuf;

use crate::{
    assets::{
        meta::{MetaData, MetaFile},
        CurrentAssetPath, EditorAssets,
    },
    tasks::{EditorTask, TaskConfirmation},
};

use super::TaskState;

pub struct ModelImportTask {
    src_path: PathBuf,
    active_package: PathBuf,
    raw_dst_path: PathBuf,
    meta_rel_path: PathBuf,
    meta_dst_path: PathBuf,
    new_assets: Vec<AssetNameBuf>,
    state: TaskState,
}

impl ModelImportTask {
    pub fn new(path: PathBuf) -> Self {
        Self {
            state: TaskState::new(format!("Import {:?}", path)),
            src_path: path,
            active_package: PathBuf::default(),
            raw_dst_path: PathBuf::default(),
            meta_rel_path: PathBuf::default(),
            meta_dst_path: PathBuf::default(),
            new_assets: Vec::default(),
        }
    }
}

impl EditorTask for ModelImportTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        let mut res = TaskConfirmation::Wait;

        ui.label(format!(
            "Do you want to import `{}`?",
            self.src_path.display()
        ));

        if ui.button("Yes").clicked() {
            res = TaskConfirmation::Ready;
        }

        if ui.button("No").clicked() {
            res = TaskConfirmation::Cancel;
        }

        Ok(res)
    }

    fn state(&mut self) -> Option<TaskState> {
        Some(self.state.clone())
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let editor_assets = res.get::<EditorAssets>().unwrap();
        let cur_path = res.get::<CurrentAssetPath>().unwrap();

        self.active_package = editor_assets.active_package_root().into();
        match self.src_path.file_name() {
            Some(file_name) => {
                self.raw_dst_path =
                    path!(editor_assets.active_assets_root() / cur_path.path() / file_name);
                self.meta_rel_path = path!(cur_path.path() / file_name);
                self.meta_rel_path.set_extension("glb.meta");
                self.meta_dst_path = path!(editor_assets.active_assets_root() / self.meta_rel_path);
            }
            None => return Err(anyhow::Error::msg("Invalid file name.")),
        }

        if editor_assets
            .find_asset(Utf8Path::from_path(&self.meta_rel_path).unwrap_or(Utf8Path::new("")))
            .is_some()
        {
            return Err(anyhow::Error::msg(
                "TODO: Can't currently import over existing asset.",
            ));
        }

        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        info!("Importing `{}`...", self.src_path.display());

        // Create a temporary folder for artifacts
        let temp_folder = tempfile::TempDir::new()?;

        // Run the command
        let in_path = format!("{}", self.src_path.display());
        let out_path = format!("{}", temp_folder.path().display());

        let output = std::process::Command::new("./tools/gltf-oven")
            .args([
                "--path",
                &in_path,
                "--out",
                &out_path,
                "--compress-textures",
                "--uuid-names",
            ])
            .output()?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(anyhow::Error::msg(err_msg));
        }

        self.state.set_completion(0.33);

        // Find the primary model asset
        let mut model_file = None;
        for entry in temp_folder.path().read_dir()? {
            let entry = entry?;
            self.new_assets
                .push(AssetNameBuf::from(entry.file_name().into_string().unwrap()));
            let path = entry.path();
            let ext = match path.extension() {
                Some(ext) => ext,
                None => continue,
            };

            if ext == "ard_mdl" {
                model_file = Some(entry.file_name());
            }
        }

        let model_file = match model_file {
            Some(model_file) => model_file,
            None => return Err(anyhow::Error::msg("could not find model file")),
        };

        // Move artifacts into the package
        let folder = temp_folder.into_path();
        fs_extra::dir::move_dir(
            folder,
            &self.active_package,
            &fs_extra::dir::CopyOptions {
                overwrite: true,
                content_only: true,
                ..Default::default()
            },
        )?;
        self.state.set_completion(0.66);

        // Copy raw asset into the assets folder
        std::fs::copy(in_path, &self.raw_dst_path)?;

        // Create the meta file for the asset
        let meta = MetaFile {
            baked: model_file.to_str().unwrap().to_owned().into(),
            data: MetaData::Model,
        };

        let file = std::fs::File::create(&self.meta_dst_path)?;
        let writer = std::io::BufWriter::new(file);
        ron::ser::to_writer(writer, &meta)?;

        self.state.set_completion(1.0);

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();
        let assets = res.get::<Assets>().unwrap();

        self.new_assets.drain(..).for_each(|new_asset| {
            assets.scan_for(&new_asset);
        });

        editor_assets
            .scan_for(Utf8Path::from_path(&self.meta_dst_path).unwrap())
            .unwrap();

        println!("Task complete...");

        Ok(())
    }
}
