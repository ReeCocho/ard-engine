use std::{
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::Result;
use ard_engine::{
    assets::{asset::AssetNameBuf, manager::Assets},
    ecs::prelude::*,
};
use camino::Utf8PathBuf;
use path_macro::path;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    assets::{
        meta::MetaFile,
        op::{AssetHeader, AssetNameGenerator, AssetOps, DeleteContext, RenameContext},
        EditorAsset, EditorAssets,
    },
    gui::util,
    refresher::RefreshAsset,
    tasks::TaskConfirmation,
};

use super::EditorTask;

pub struct RenameAssetTask {
    // Root of the assets folder relative to the executable
    assets_root: Utf8PathBuf,
    // Path to the meta file relative to the assets folder
    meta_path: Utf8PathBuf,
    // Path to the raw asset relative to the assets folder
    raw_path: Utf8PathBuf,
    // Name of the baked asset relative to the package root
    baked_path: AssetNameBuf,
    new_name: String,
    is_shadowing: bool,
    ctx: Option<RenameContext>,
}

pub struct MoveAssetTask {
    // Root of the assets folder relative to the executable
    assets_root: Utf8PathBuf,
    // Path to the meta file relative to the assets folder
    meta_path: Utf8PathBuf,
    // Path to the raw asset relative to the assets folder
    raw_path: Utf8PathBuf,
    // Name of the baked asset relative to the package root
    baked_path: AssetNameBuf,
    new_folder: Utf8PathBuf,
    is_shadowing: bool,
    ctx: Option<RenameContext>,
}

pub struct DeleteAssetTask {
    // Root of the assets folder relative to the executable
    assets_root: Utf8PathBuf,
    // Path to the meta file relative to the assets folder
    meta_path: Utf8PathBuf,
    // Path to the raw asset relative to the assets folder
    raw_path: Utf8PathBuf,
    // Name of the baked asset relative to the package root
    baked_path: AssetNameBuf,
    is_leaf: bool,
    ctx: Option<DeleteContext>,
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

impl RenameAssetTask {
    pub fn new(asset: &EditorAsset) -> Self {
        Self {
            assets_root: Utf8PathBuf::default(),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            new_name: String::default(),
            is_shadowing: false,
            ctx: None,
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

impl MoveAssetTask {
    pub fn new(asset: &EditorAsset, new_folder: impl Into<Utf8PathBuf>) -> Self {
        Self {
            assets_root: Utf8PathBuf::default(),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            new_folder: new_folder.into(),
            is_shadowing: false,
            ctx: None,
        }
    }

    fn new_raw_path(&self) -> PathBuf {
        let name = self.raw_path.file_name().unwrap_or("");
        let mut dst_raw = self.new_folder.clone();
        dst_raw.push(name);
        path!(self.assets_root / dst_raw)
    }

    fn new_meta_path(&self) -> PathBuf {
        let name = self.meta_path.file_name().unwrap_or("");
        let mut dst_meta = self.new_folder.clone();
        dst_meta.push(name);
        path!(self.assets_root / dst_meta)
    }

    fn new_meta_rel_path(&self) -> Utf8PathBuf {
        let name = self.meta_path.file_name().unwrap_or("");
        let mut dst_meta = self.new_folder.clone();
        dst_meta.push(name);
        dst_meta
    }
}

impl DeleteAssetTask {
    pub fn new(asset: &EditorAsset) -> Self {
        Self {
            assets_root: Utf8PathBuf::default(),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            is_leaf: false,
            ctx: None,
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

impl EditorTask for RenameAssetTask {
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
        let assets = res.get::<Assets>().unwrap();
        let editor_assets = res.get::<EditorAssets>().unwrap();

        let asset = editor_assets
            .find_asset(&self.meta_path)
            .ok_or(anyhow::Error::msg("Asset no longer exists"))?;
        self.is_shadowing = asset.is_shadowing();
        self.baked_path = asset.meta_file().baked.clone();

        self.assets_root = editor_assets.active_assets_root().into();
        self.ctx = Some(RenameContext {
            package_root: editor_assets.active_package_root().into(),
            old_to_new: FxHashMap::default(),
            gen: AssetNameGenerator::new(assets.clone()),
        });

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

    fn run(&mut self) -> Result<()> {
        let src_raw = path!(self.assets_root / self.raw_path);
        let src_meta = path!(self.assets_root / self.meta_path);

        let dst_raw = self.new_raw_path();
        let dst_meta = self.new_meta_path();

        // If it's shadowed, we need to generate unique names for all assets
        if self.is_shadowing {
            let ctx = self.ctx.as_mut().unwrap();
            let baked_path = path!(ctx.package_root / self.baked_path);
            let mut header = AssetHeader::load(baked_path)?;

            let f = BufReader::new(std::fs::File::open(&src_meta)?);
            let mut meta_file = ron::de::from_reader::<_, MetaFile>(f)?;
            meta_file.baked = header.rename(&self.baked_path, ctx)?;

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
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();

        if self.is_shadowing {
            self.ctx
                .as_mut()
                .unwrap()
                .old_to_new
                .iter()
                .for_each(|(old, new)| {
                    assets.scan_for(old);
                    assets.scan_for(new);
                });
        }

        editor_assets.remove_from_active_package(&self.meta_path);
        editor_assets.scan_for(Utf8PathBuf::from_path_buf(self.new_meta_path()).unwrap())?;

        Ok(())
    }
}

impl EditorTask for DeleteAssetTask {
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

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let editor_assets = res.get::<EditorAssets>().unwrap();
        self.assets_root = editor_assets.active_assets_root().into();

        // Check for asset
        match editor_assets.find_asset(&self.meta_path) {
            Some(asset) => {
                self.is_leaf = asset.package() == editor_assets.active_package_id();
                self.baked_path = asset.meta_file().baked.clone();
                self.ctx = Some(DeleteContext {
                    package_root: editor_assets.active_package_root().into(),
                    visited: FxHashSet::default(),
                });
                Ok(())
            }
            None => Err(anyhow::Error::msg("Asset does not exists.")),
        }
    }

    fn run(&mut self) -> Result<()> {
        let src_raw = path!(self.assets_root / self.raw_path);
        let src_meta = path!(self.assets_root / self.meta_path);

        if !src_meta.exists() || !src_raw.exists() {
            return Err(anyhow::Error::msg("Asset no longer exists."));
        }

        let ctx = self.ctx.as_mut().unwrap();
        let mut header = AssetHeader::load(path!(ctx.package_root / self.baked_path))?;
        header.delete(&self.baked_path, ctx)?;

        std::fs::remove_file(src_raw)?;
        std::fs::remove_file(src_meta)?;

        Ok(())
    }

    fn complete(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();

        if self.is_leaf {
            let ctx = self.ctx.as_ref().unwrap();
            ctx.visited.iter().for_each(|asset| {
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

impl EditorTask for MoveAssetTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        Ok(TaskConfirmation::Wait)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let editor_assets = res.get::<EditorAssets>().unwrap();

        let asset = editor_assets
            .find_asset(&self.meta_path)
            .ok_or(anyhow::Error::msg("Asset no longer exists"))?;
        self.is_shadowing = asset.is_shadowing();
        self.baked_path = asset.meta_file().baked.clone();

        self.assets_root = editor_assets.active_assets_root().into();
        self.ctx = Some(RenameContext {
            package_root: editor_assets.active_package_root().into(),
            old_to_new: FxHashMap::default(),
            gen: AssetNameGenerator::new(assets.clone()),
        });

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

    fn run(&mut self) -> Result<()> {
        let src_raw = path!(self.assets_root / self.raw_path);
        let src_meta = path!(self.assets_root / self.meta_path);

        let dst_raw = self.new_raw_path();
        let dst_meta = self.new_meta_path();

        // If it's shadowed, we need to generate unique names for all assets
        if self.is_shadowing {
            let ctx = self.ctx.as_mut().unwrap();
            let baked_path = path!(ctx.package_root / self.baked_path);
            let mut header = AssetHeader::load(baked_path)?;

            let f = BufReader::new(std::fs::File::open(&src_meta)?);
            let mut meta_file = ron::de::from_reader::<_, MetaFile>(f)?;
            meta_file.baked = header.rename(&self.baked_path, ctx)?;

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
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();

        if self.is_shadowing {
            self.ctx
                .as_mut()
                .unwrap()
                .old_to_new
                .iter()
                .for_each(|(old, new)| {
                    assets.scan_for(old);
                    assets.scan_for(new);
                });
        }

        editor_assets.remove_from_active_package(&self.meta_path);
        editor_assets.scan_for(Utf8PathBuf::from_path_buf(self.new_meta_path()).unwrap())?;

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
