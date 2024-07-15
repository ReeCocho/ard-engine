use std::{
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use anyhow::Result;
use ard_engine::{
    assets::{
        asset::{AssetName, AssetNameBuf},
        manager::Assets,
    },
    formats::{
        material::{MaterialHeader, MaterialType},
        mesh::MeshHeader,
        model::ModelHeader,
        texture::TextureHeader,
    },
};
use camino::{Utf8Path, Utf8PathBuf};
use path_macro::path;
use rustc_hash::FxHashMap;

use crate::{
    assets::{
        meta::{AssetType, MetaFile},
        op::AssetOp,
        EditorAsset, EditorAssets,
    },
    gui::util,
    tasks::TaskConfirmation,
};

pub struct RenameAssetOp {
    assets_root: Utf8PathBuf,
    package_root: PathBuf,
    meta_path: Utf8PathBuf,
    raw_path: Utf8PathBuf,
    baked_path: AssetNameBuf,
    new_name: String,
    asset_ty: AssetType,
    assets: Option<Assets>,
    old_to_new: FxHashMap<AssetNameBuf, AssetNameBuf>,
}

impl RenameAssetOp {
    pub fn new(assets: &EditorAssets, asset: &EditorAsset) -> Self {
        Self {
            assets_root: assets.active_assets_root().into(),
            package_root: assets.active_package_root().into(),
            meta_path: asset.meta_path().into(),
            baked_path: asset.meta_file().baked.clone(),
            raw_path: asset.raw_path(),
            asset_ty: asset.meta_file().data.ty(),
            new_name: String::default(),
            assets: None,
            old_to_new: FxHashMap::default(),
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

impl AssetOp for RenameAssetOp {
    fn meta_path(&self) -> &Utf8Path {
        &self.meta_path
    }

    fn raw_path(&self) -> &Utf8Path {
        &self.raw_path
    }

    fn asset_ty(&self) -> AssetType {
        self.asset_ty
    }

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

    fn pre_run(&mut self, assets: &Assets, editor_assets: &EditorAssets) -> Result<()> {
        self.assets_root = editor_assets.active_assets_root().into();
        self.package_root = editor_assets.active_package_root().into();
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
            create_unique_names_for(
                &self.package_root,
                &self.baked_path,
                self.asset_ty,
                &mut self.old_to_new,
                assets,
            )?;

            let f = BufReader::new(std::fs::File::open(&src_meta)?);
            let mut meta_file = ron::de::from_reader::<_, MetaFile>(f)?;
            meta_file.baked = self.old_to_new.get(&self.baked_path).unwrap().clone();

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
        assets: &Assets,
        editor_assets: &mut EditorAssets,
    ) -> Result<()> {
        if is_shadowing {
            self.old_to_new.iter().for_each(|(old, new)| {
                assets.scan_for(old);
                assets.scan_for(new);
            });
        }

        editor_assets.remove_from_active_package(&self.meta_path);
        editor_assets.scan_for(Utf8PathBuf::from_path_buf(self.new_meta_path()).unwrap())?;

        Ok(())
    }
}

fn create_unique_names_for(
    package_root: &Path,
    baked: &AssetName,
    ty: AssetType,
    old_to_new: &mut FxHashMap<AssetNameBuf, AssetNameBuf>,
    assets: &Assets,
) -> anyhow::Result<()> {
    match ty {
        AssetType::Model => create_unique_name_for_model(package_root, baked, old_to_new, assets),
    }
}

fn create_unique_name_for_model(
    package_root: &Path,
    baked: &AssetName,
    old_to_new: &mut FxHashMap<AssetNameBuf, AssetNameBuf>,
    assets: &Assets,
) -> anyhow::Result<()> {
    if old_to_new.contains_key(baked) {
        return Ok(());
    }

    let new_name = create_unique_name(assets, baked);

    let header_path = path!(package_root / baked);
    let f = BufReader::new(std::fs::File::open(&header_path)?);
    let mut header = bincode::deserialize_from::<_, ModelHeader>(f)?;

    for texture in &mut header.textures {
        create_unique_name_for_texture(package_root, texture, old_to_new, assets)?;
        *texture = old_to_new.get(texture).unwrap().clone();
    }

    for mesh in &mut header.meshes {
        create_unique_name_for_mesh(package_root, mesh, old_to_new, assets)?;
        *mesh = old_to_new.get(mesh).unwrap().clone();
    }

    for material in &mut header.materials {
        create_unique_name_for_material(package_root, material, old_to_new, assets)?;
        *material = old_to_new.get(material).unwrap().clone();
    }

    let mut f = BufWriter::new(std::fs::File::create(path!(package_root / new_name))?);
    bincode::serialize_into(&mut f, &header).unwrap();
    std::fs::remove_file(header_path)?;

    old_to_new.insert(baked.into(), new_name);

    Ok(())
}

fn create_unique_name_for_texture(
    package_root: &Path,
    baked: &AssetName,
    old_to_new: &mut FxHashMap<AssetNameBuf, AssetNameBuf>,
    assets: &Assets,
) -> anyhow::Result<()> {
    if old_to_new.contains_key(baked) {
        return Ok(());
    }

    let new_name = create_unique_name(assets, baked);

    let header_path = path!(package_root / baked);
    let f = BufReader::new(std::fs::File::open(&header_path)?);
    let mut header = bincode::deserialize_from::<_, TextureHeader>(f)?;

    for mip in &mut header.mips {
        let new_mip_name = create_unique_name(assets, mip);
        let src = path!(package_root / mip);
        let dst = path!(package_root / new_mip_name);
        std::fs::rename(src, dst)?;
        old_to_new.insert(mip.clone(), new_mip_name.clone());
        *mip = new_mip_name;
    }

    let mut f = BufWriter::new(std::fs::File::create(path!(package_root / new_name))?);
    bincode::serialize_into(&mut f, &header).unwrap();
    std::fs::remove_file(header_path)?;

    old_to_new.insert(baked.into(), new_name);

    Ok(())
}

fn create_unique_name_for_mesh(
    package_root: &Path,
    baked: &AssetName,
    old_to_new: &mut FxHashMap<AssetNameBuf, AssetNameBuf>,
    assets: &Assets,
) -> anyhow::Result<()> {
    if old_to_new.contains_key(baked) {
        return Ok(());
    }

    let new_name = create_unique_name(assets, baked);

    let header_path = path!(package_root / baked);
    let f = BufReader::new(std::fs::File::open(&header_path)?);
    let mut header = bincode::deserialize_from::<_, MeshHeader>(f)?;

    let new_data_name = create_unique_name(assets, &header.data_path);
    let src = path!(package_root / header.data_path);
    let dst = path!(package_root / new_data_name);
    std::fs::rename(src, dst)?;

    old_to_new.insert(header.data_path.clone(), new_data_name.clone());
    header.data_path = new_data_name;

    let mut f = BufWriter::new(std::fs::File::create(path!(package_root / new_name))?);
    bincode::serialize_into(&mut f, &header).unwrap();
    std::fs::remove_file(header_path)?;

    old_to_new.insert(baked.into(), new_name);

    Ok(())
}

fn create_unique_name_for_material(
    package_root: &Path,
    baked: &AssetName,
    old_to_new: &mut FxHashMap<AssetNameBuf, AssetNameBuf>,
    assets: &Assets,
) -> anyhow::Result<()> {
    if old_to_new.contains_key(baked) {
        return Ok(());
    }

    let new_name = create_unique_name(assets, baked);

    let header_path = path!(package_root / baked);
    let f = BufReader::new(std::fs::File::open(&header_path)?);
    let mut header = bincode::deserialize_from::<_, MaterialHeader<AssetNameBuf>>(f)?;

    match &mut header.ty {
        MaterialType::Pbr {
            diffuse_map,
            normal_map,
            metallic_roughness_map,
            ..
        } => {
            if let Some(tex) = diffuse_map {
                create_unique_name_for_texture(package_root, tex, old_to_new, assets)?;
                *tex = old_to_new.get(tex).unwrap().clone();
            }

            if let Some(tex) = normal_map {
                create_unique_name_for_texture(package_root, tex, old_to_new, assets)?;
                *tex = old_to_new.get(tex).unwrap().clone();
            }

            if let Some(tex) = metallic_roughness_map {
                create_unique_name_for_texture(package_root, tex, old_to_new, assets)?;
                *tex = old_to_new.get(tex).unwrap().clone();
            }
        }
    }

    let mut f = BufWriter::new(std::fs::File::create(path!(package_root / new_name))?);
    bincode::serialize_into(&mut f, &header).unwrap();
    std::fs::remove_file(header_path)?;

    old_to_new.insert(baked.into(), new_name);

    Ok(())
}

fn create_unique_name(assets: &Assets, src: &AssetName) -> AssetNameBuf {
    let ext = src.extension().unwrap_or("");
    loop {
        let mut new_name = AssetNameBuf::from(uuid::Uuid::new_v4().to_string());
        new_name.set_extension(ext);

        if assets.exists(&new_name) {
            continue;
        }

        break new_name;
    }
}

/*
pub struct NewFolderTask {
    parent: PathBuf,
    package_root: PathBuf,
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

pub struct MoveTask {
    src: PathBuf,
    dst_dir: PathBuf,
}

impl NewFolderTask {
    pub fn new(parent: impl Into<PathBuf>, package_root: impl Into<PathBuf>) -> Self {
        Self {
            parent: parent.into(),
            package_root: package_root.into(),
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

impl MoveTask {
    pub fn new(src: impl Into<PathBuf>, dst_dir: impl Into<PathBuf>) -> Self {
        Self {
            src: src.into(),
            dst_dir: dst_dir.into(),
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
        let final_path = path!(self.package_root / self.parent / self.name);
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
            .strip_prefix(assets.active_package_path())
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
            .strip_prefix(assets.active_package_path())
            .unwrap();
        assets
            .find_folder_mut(&parent_folder)
            .map(|folder| folder.inspect());
        Ok(())
    }
}

impl EditorTask for MoveTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        Ok(TaskConfirmation::Ready)
    }

    fn run(&mut self) -> Result<()> {
        let file_name = match self.src.file_name() {
            Some(file_name) => file_name,
            None => return Err(anyhow::Error::msg("File had no name.")),
        };

        let dst_path = path!(self.dst_dir / file_name);
        Ok(std::fs::rename(&self.src, dst_path)?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();
        let parent_dir = self.src.parent().map(|p| p.to_owned()).unwrap_or_default();
        let parent_dir = parent_dir
            .strip_prefix(editor_assets.active_package_path())
            .map(|d| d.to_path_buf())
            .unwrap_or_default();

        editor_assets
            .find_folder_mut(parent_dir)
            .map(|folder| folder.inspect());

        Ok(())
    }
}
*/
