use anyhow::Result;
use ard_engine::ecs::prelude::*;
use path_macro::path;
use std::path::PathBuf;

use crate::{
    assets::{EditorAssets, FolderAsset, ASSETS_FOLDER},
    gui::util,
    tasks::TaskConfirmation,
};

use super::EditorTask;

pub struct NewFolderTask {
    parent: PathBuf,
    name: String,
}

pub struct DestroyFolderTask {
    path: PathBuf,
}

pub struct RenameAssetTask {
    asset: FolderAsset,
    new_name: String,
}

pub struct RenameFolderTask {
    path: PathBuf,
    new_name: String,
}

impl NewFolderTask {
    pub fn new(parent: impl Into<PathBuf>) -> Self {
        Self {
            parent: parent.into(),
            name: String::default(),
        }
    }
}

impl DestroyFolderTask {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl RenameAssetTask {
    pub fn new(asset: FolderAsset) -> Self {
        Self {
            new_name: asset
                .raw_path
                .file_name()
                .and_then(|osstr| osstr.to_str())
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            asset,
        }
    }
}

impl RenameFolderTask {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path: PathBuf = path.into();
        Self {
            new_name: path
                .file_name()
                .and_then(|osstr| osstr.to_str())
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            path,
        }
    }
}

impl EditorTask for NewFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.label("Folder Name:");
        ui.text_edit_singleline(&mut self.name);

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        if ui
            .add_enabled(!self.name.is_empty(), util::constructive_button("Create"))
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn run(&mut self) -> Result<()> {
        let final_path = path!(ASSETS_FOLDER / "main" / self.parent / self.name);
        Ok(std::fs::create_dir_all(final_path)?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        assets
            .find_folder_mut(&self.parent)
            .map(|folder| folder.inspect());
        Ok(())
    }
}

impl EditorTask for DestroyFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.label(format!("Are you sure you want to destroy `{}`? This operation is permanent, and cannot be undone. All assets contained within the folder will be destroyed", self.path.display()));

        if ui.add(util::destructive_button("Yes")).clicked() {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("No").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn run(&mut self) -> Result<()> {
        Err(anyhow::Error::msg("TODO"))
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut parent = self.path.clone();
        parent.pop();

        res.get_mut::<EditorAssets>()
            .unwrap()
            .find_folder_mut(&parent)
            .map(|path| path.inspect());
        Ok(())
    }
}

impl EditorTask for RenameAssetTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.new_name);

        if ui
            .add_enabled(
                !self.new_name.is_empty(),
                util::transformation_button("Rename"),
            )
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn run(&mut self) -> Result<()> {
        let parent_folder = self
            .asset
            .raw_path
            .parent()
            .map(|p| p.to_owned())
            .unwrap_or_default();

        let new_raw = path!(parent_folder / self.new_name);
        let mut new_meta = new_raw.clone();
        new_meta.set_extension(
            new_raw
                .extension()
                .map(|ext| {
                    let mut ext = ext.to_os_string();
                    ext.push(".meta");
                    ext
                })
                .unwrap_or_else(|| "meta".into()),
        );

        if self.asset.raw_path.extension() != new_raw.extension() {
            return Err(anyhow::Error::msg(
                "Renamed asset must have the same extension.",
            ));
        }

        if new_raw.exists() {
            return Err(anyhow::Error::msg("Asset with that name already exists."));
        }

        std::fs::rename(&self.asset.raw_path, &new_raw)?;
        std::fs::rename(&self.asset.meta_path, &new_meta)?;

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        let parent_folder = self
            .asset
            .raw_path
            .parent()
            .map(|p| p.to_owned())
            .unwrap_or_default();
        let parent_folder = parent_folder
            .strip_prefix(path!(ASSETS_FOLDER / "main"))
            .unwrap();
        assets
            .find_folder_mut(&parent_folder)
            .map(|folder| folder.inspect());
        Ok(())
    }
}

impl EditorTask for RenameFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.new_name);

        if ui
            .add_enabled(
                !self.new_name.is_empty(),
                util::transformation_button("Rename"),
            )
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn run(&mut self) -> Result<()> {
        let mut new_path = self.path.clone();
        new_path.set_file_name(&self.new_name);

        if new_path.exists() {
            return Err(anyhow::Error::msg("Folder with that name already exists"));
        }

        Ok(std::fs::rename(&self.path, &new_path)?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        let parent_folder = self.path.parent().map(|p| p.to_owned()).unwrap_or_default();
        let parent_folder = parent_folder
            .strip_prefix(path!(ASSETS_FOLDER / "main"))
            .unwrap();
        assets
            .find_folder_mut(&parent_folder)
            .map(|folder| folder.inspect());
        Ok(())
    }
}
