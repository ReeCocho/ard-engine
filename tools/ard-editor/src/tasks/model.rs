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
        EditorAssets, ASSETS_FOLDER,
    },
    tasks::{EditorTask, TaskConfirmation},
};

pub struct ModelImportTask {
    path: PathBuf,
    meta_file_path: PathBuf,
    new_assets: Vec<AssetNameBuf>,
}

impl ModelImportTask {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            meta_file_path: PathBuf::default(),
            new_assets: Vec::default(),
        }
    }
}

impl EditorTask for ModelImportTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        let mut res = TaskConfirmation::Wait;

        ui.label(format!("Do you want to import `{}`?", self.path.display()));

        if ui.button("Yes").clicked() {
            res = TaskConfirmation::Ready;
        }

        if ui.button("No").clicked() {
            res = TaskConfirmation::Cancel;
        }

        Ok(res)
    }

    fn run(&mut self) -> Result<()> {
        info!("Importing `{}`...", self.path.display());

        // Create a temporary folder for artifacts
        let temp_folder = tempfile::TempDir::new()?;

        // Run the command
        let in_path = format!("{}", self.path.display());
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
            "./packages/main",
            &fs_extra::dir::CopyOptions {
                overwrite: true,
                content_only: true,
                ..Default::default()
            },
        )?;

        // Copy raw asset into the assets folder
        let (out_path, meta_path) = match self.path.file_name() {
            Some(file_name) => {
                let out_path = path!("./main" / file_name);
                let mut meta_path = out_path.clone();
                meta_path.set_extension("glb.meta");

                (out_path, meta_path)
            }
            None => return Err(anyhow::Error::msg("Invalid file name.")),
        };

        std::fs::copy(in_path, path!(ASSETS_FOLDER / out_path))?;

        // Create the meta file for the asset
        let meta = MetaFile {
            baked: model_file.to_str().unwrap().to_owned().into(),
            data: MetaData::Model,
        };

        let meta_file_path = path!(ASSETS_FOLDER / meta_path);
        let file = std::fs::File::create(&meta_file_path)?;
        let writer = std::io::BufWriter::new(file);
        ron::ser::to_writer(writer, &meta)?;

        self.meta_file_path = meta_file_path;

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
            .scan_for(Utf8Path::from_path(&self.meta_file_path).unwrap())
            .unwrap();

        println!("Task complete...");

        Ok(())
    }
}
