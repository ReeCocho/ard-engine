pub mod delete;
pub mod rename;

use std::{
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::Result;
use ard_engine::{
    assets::{asset::AssetNameBuf, manager::Assets},
    ecs::prelude::*,
};
use camino::{Utf8Path, Utf8PathBuf};
use delete::DeleteAssetVisitor;
use path_macro::path;
use rename::RenameAssetVisitor;

use crate::{
    assets::{meta::MetaFile, op::AssetOp, EditorAsset, EditorAssets},
    gui::util,
    refresher::RefreshAsset,
    tasks::TaskConfirmation,
};

use super::EditorTask;

pub struct RenameAssetOp {
    assets_root: Utf8PathBuf,
    meta_path: Utf8PathBuf,
    raw_path: Utf8PathBuf,
    baked_path: AssetNameBuf,
    new_name: String,
    visitor: RenameAssetVisitor,
    assets: Option<Assets>,
}

pub struct DeleteAssetOp {
    assets_root: Utf8PathBuf,
    meta_path: Utf8PathBuf,
    raw_path: Utf8PathBuf,
    baked_path: AssetNameBuf,
    visitor: DeleteAssetVisitor,
}

pub struct NewFolderTask {
    active_assets: Utf8PathBuf,
    parent: Utf8PathBuf,
    folder_name: String,
}

pub struct DeleteFolderTask {
    active_assets: Utf8PathBuf,
    folder: Utf8PathBuf,
}

pub struct RenameFolderTask {
    active_assets: Utf8PathBuf,
    old_rel_path: Utf8PathBuf,
    new_name: String,
}

impl RenameAssetOp {
    pub fn new(assets: &EditorAssets, asset: &EditorAsset) -> Self {
        Self {
            assets_root: assets.active_assets_root().into(),
            visitor: RenameAssetVisitor::new(assets.active_package_root()),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            new_name: String::default(),
            assets: None,
        }
    }

    fn new_raw_path(&self) -> PathBuf {
        let ext = self.raw_path.extension().unwrap_or("");
        let mut dst_raw = self.raw_path.clone();
        dst_raw.set_file_name(&self.new_name);
        dst_raw.set_extension(ext);
        path!(self.assets_root / dst_raw)
    }

    fn new_meta_path(&self) -> PathBuf {
        let ext = self.raw_path.extension().unwrap_or("");
        let mut dst_meta = self.meta_path.clone();
        dst_meta.set_file_name(&self.new_name);
        dst_meta.set_extension(format!("{ext}.meta"));
        path!(self.assets_root / dst_meta)
    }

    fn new_meta_rel_path(&self) -> Utf8PathBuf {
        let ext = self.raw_path.extension().unwrap_or("");
        let mut dst_meta = self.meta_path.clone();
        dst_meta.set_file_name(&self.new_name);
        dst_meta.set_extension(format!("{ext}.meta"));
        dst_meta
    }
}

impl DeleteAssetOp {
    pub fn new(assets: &EditorAssets, asset: &EditorAsset) -> Self {
        Self {
            assets_root: assets.active_assets_root().into(),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            visitor: DeleteAssetVisitor::new(assets.active_package_root()),
        }
    }
}

impl NewFolderTask {
    pub fn new(parent: impl Into<Utf8PathBuf>) -> Self {
        Self {
            parent: parent.into(),
            folder_name: String::default(),
            active_assets: Utf8PathBuf::default(),
        }
    }

    pub fn new_folder_path_rel(&self) -> Utf8PathBuf {
        let mut out = self.parent.clone();
        out.push(&self.folder_name);
        out
    }

    pub fn new_folder_path(&self) -> Utf8PathBuf {
        let mut out = self.active_assets.clone();
        out.push(self.new_folder_path_rel());
        out
    }
}

impl DeleteFolderTask {
    pub fn new(folder: impl Into<Utf8PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            active_assets: Utf8PathBuf::default(),
        }
    }

    pub fn folder_path(&self) -> Utf8PathBuf {
        let mut out = self.active_assets.clone();
        out.push(&self.folder);
        out
    }
}

impl RenameFolderTask {
    pub fn new(folder: impl Into<Utf8PathBuf>) -> Self {
        Self {
            old_rel_path: folder.into(),
            new_name: String::default(),
            active_assets: Utf8PathBuf::default(),
        }
    }

    pub fn new_rel_name(&self) -> Utf8PathBuf {
        let mut out = self.old_rel_path.clone();
        out.set_file_name(&self.new_name);
        out
    }

    pub fn old_abs_path(&self) -> Utf8PathBuf {
        let mut out = self.active_assets.clone();
        out.push(&self.old_rel_path);
        out
    }

    pub fn new_abs_path(&self) -> Utf8PathBuf {
        let mut out = self.active_assets.clone();
        out.push(&self.old_rel_path);
        out.set_file_name(&self.new_name);
        out
    }
}

impl AssetOp for RenameAssetOp {
    fn meta_path(&self) -> &Utf8Path {
        &self.meta_path
    }

    fn raw_path(&self) -> &Utf8Path {
        &self.raw_path
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.new_name);

        let mut valid_name = !self.new_name.is_empty();
        for c in self.new_name.chars() {
            if std::path::is_separator(c) {
                valid_name = false;
                break;
            }
        }

        if ui
            .add_enabled(valid_name, util::transformation_button("Rename"))
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(&mut self, assets: &Assets, editor_assets: &EditorAssets) -> Result<()> {
        self.assets_root = editor_assets.active_assets_root().into();
        self.visitor = RenameAssetVisitor::new(editor_assets.active_package_root());
        self.assets = Some(assets.clone());

        // Check for duplicate
        if editor_assets
            .find_asset(&self.new_meta_rel_path())
            .is_some()
        {
            Err(anyhow::Error::msg("Asset already exists."))
        } else {
            Ok(())
        }
    }

    fn run(&mut self, is_shadowing: bool, _is_leaf: bool) -> Result<()> {
        let assets = self.assets.as_ref().unwrap();

        let src_raw = path!(self.assets_root / self.raw_path);
        let src_meta = path!(self.assets_root / self.meta_path);

        let dst_raw = self.new_raw_path();
        let dst_meta = self.new_meta_path();

        // If it's shadowed, we need to generate unique names for all assets
        if is_shadowing {
            let f = BufReader::new(std::fs::File::open(&src_meta)?);
            let mut meta_file = ron::de::from_reader::<_, MetaFile>(f)?;
            meta_file.baked = self.visitor.visit(assets, &self.baked_path)?;

            let mut f = BufWriter::new(std::fs::File::create(&src_meta)?);
            ron::ser::to_writer(&mut f, &meta_file)?;
        }

        // Perform the rename on the raw and meta files
        std::fs::rename(src_raw, dst_raw)?;
        std::fs::rename(src_meta, dst_meta)?;

        Ok(())
    }

    fn complete(
        &mut self,
        is_shadowing: bool,
        _is_leaf: bool,
        _commands: &Commands,
        assets: &Assets,
        editor_assets: &mut EditorAssets,
    ) -> Result<()> {
        if is_shadowing {
            self.visitor.scan(assets);
        }

        editor_assets.remove_from_active_package(&self.meta_path);
        editor_assets.scan_for(Utf8PathBuf::from_path_buf(self.new_meta_path()).unwrap())?;

        Ok(())
    }
}

impl AssetOp for DeleteAssetOp {
    fn meta_path(&self) -> &Utf8Path {
        &self.meta_path
    }

    fn raw_path(&self) -> &Utf8Path {
        &self.raw_path
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.label(format!(
            "Are you sure you want to delete `{}`? This operation cannot be undone.",
            self.raw_path
        ));

        if ui.add(util::destructive_button("Delete")).clicked() {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(&mut self, _assets: &Assets, editor_assets: &EditorAssets) -> Result<()> {
        self.assets_root = editor_assets.active_assets_root().into();
        self.visitor = DeleteAssetVisitor::new(editor_assets.active_package_root());

        // Check for asset
        if editor_assets.find_asset(&self.meta_path).is_none() {
            Err(anyhow::Error::msg("Asset does not exists."))
        } else {
            Ok(())
        }
    }

    fn run(&mut self, _is_shadowing: bool, _is_leaf: bool) -> Result<()> {
        let src_raw = path!(self.assets_root / self.raw_path);
        let src_meta = path!(self.assets_root / self.meta_path);

        if !src_meta.exists() || !src_raw.exists() {
            return Err(anyhow::Error::msg("Asset no longer exists."));
        }

        self.visitor.visit(&self.baked_path)?;

        std::fs::remove_file(src_raw)?;
        std::fs::remove_file(src_meta)?;

        Ok(())
    }

    fn complete(
        &mut self,
        _is_shadowing: bool,
        is_leaf: bool,
        commands: &Commands,
        assets: &Assets,
        editor_assets: &mut EditorAssets,
    ) -> Result<()> {
        if is_leaf {
            self.visitor.visited().iter().for_each(|asset| {
                assets.scan_for(&asset);
                commands.events.submit(RefreshAsset(asset.clone()));
            });
        }

        editor_assets.remove_from_active_package(&self.meta_path);

        Ok(())
    }
}

impl EditorTask for NewFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.folder_name);

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        let mut valid_name = !self.folder_name.is_empty();
        for c in self.folder_name.chars() {
            if std::path::is_separator(c) {
                valid_name = false;
                break;
            }
        }

        if ui
            .add_enabled(valid_name, util::constructive_button("Create"))
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<EditorAssets>().unwrap();

        self.active_assets = assets.active_assets_root().into();
        if assets.find_folder(self.new_folder_path_rel()).is_some() {
            Err(anyhow::Error::msg("Folder with that name already exists."))
        } else {
            Ok(())
        }
    }

    fn run(&mut self) -> Result<()> {
        Ok(std::fs::create_dir_all(self.new_folder_path())?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        Ok(assets.scan_for(self.new_folder_path())?)
    }
}

impl EditorTask for DeleteFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.label(format!(
            "Are you sure you want to delete `{}`?",
            self.folder
        ));

        if ui.add(util::destructive_button("Yes")).clicked() {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("No").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<EditorAssets>().unwrap();

        self.active_assets = assets.active_assets_root().into();
        let folder = assets
            .find_folder(&self.folder)
            .ok_or_else(|| anyhow::Error::msg("Folder no longer exists."))?;

        if !self.folder_path().exists() {
            return Err(anyhow::Error::msg(
                "Folder does not exist in the active package.",
            ));
        }

        if !folder.sub_folders().is_empty() || !folder.assets().is_empty() {
            Err(anyhow::Error::msg("Folder must be empty to delete."))
        } else {
            Ok(())
        }
    }

    fn run(&mut self) -> Result<()> {
        Ok(std::fs::remove_dir(self.folder_path())?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        assets.remove_from_active_package(&self.folder);

        Ok(())
    }
}

impl EditorTask for RenameFolderTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.new_name);

        let mut valid_name = !self.new_name.is_empty();
        for c in self.new_name.chars() {
            if std::path::is_separator(c) {
                valid_name = false;
                break;
            }
        }

        if ui
            .add_enabled(valid_name, util::transformation_button("Rename"))
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<EditorAssets>().unwrap();
        self.active_assets = assets.active_assets_root().into();

        let folder = assets
            .find_folder(&self.old_rel_path)
            .ok_or_else(|| anyhow::Error::msg("Folder no longer exists."))?;

        if assets.find_folder(self.new_rel_name()).is_some() {
            return Err(anyhow::Error::msg("Folder with that name already exists."));
        }

        if !folder.contained_only_within(assets.active_package_id()) {
            return Err(anyhow::Error::msg(
                "Folder must only contain assets in the active package.",
            ));
        }

        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        Ok(std::fs::rename(self.old_abs_path(), self.new_abs_path())?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut assets = res.get_mut::<EditorAssets>().unwrap();
        assets.remove_from_active_package(&self.old_rel_path);
        Ok(assets.scan_for(self.new_abs_path())?)
    }
}
