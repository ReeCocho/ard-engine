use std::{
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::save_data::{SceneAsset, SceneAssetHeader},
    save_load::{format::Ron, save_data::SaveData},
};
use camino::Utf8PathBuf;
use path_macro::path;

use crate::{
    assets::{
        meta::{MetaData, MetaFile},
        EditorAsset, EditorAssets,
    },
    gui::util,
    scene_graph::SceneGraph,
};

use super::{EditorTask, TaskConfirmation};

pub struct SaveSceneTask {
    assets: Option<Assets>,
    name: String,
    save_data: Option<SaveData>,
    meta_file: Option<MetaFile>,
    overwrite: bool,
    assets_root: Utf8PathBuf,
    package_root: PathBuf,
    containing_folder: Utf8PathBuf,
}

impl SaveSceneTask {
    pub fn new(containing_folder: impl Into<Utf8PathBuf>) -> Self {
        Self {
            assets: None,
            name: String::default(),
            save_data: None,
            meta_file: None,
            overwrite: false,
            assets_root: Utf8PathBuf::default(),
            package_root: PathBuf::default(),
            containing_folder: containing_folder.into(),
        }
    }

    pub fn new_overwrite(asset: &EditorAsset) -> Self {
        Self {
            assets: None,
            name: asset.raw_path().file_stem().unwrap_or("").into(),
            save_data: None,
            meta_file: None,
            overwrite: true,
            assets_root: Utf8PathBuf::default(),
            package_root: PathBuf::default(),
            containing_folder: asset
                .raw_path()
                .parent()
                .map(|p| p.into())
                .unwrap_or_default(),
        }
    }

    fn new_raw_path(&self) -> PathBuf {
        let mut dst_raw = self.containing_folder.clone();
        dst_raw.push(&self.name);
        dst_raw.set_extension("save");
        path!(self.assets_root / dst_raw)
    }

    fn new_meta_path(&self) -> PathBuf {
        let mut dst_meta = self.containing_folder.clone();
        dst_meta.push(&self.name);
        dst_meta.set_extension("save.meta");
        path!(self.assets_root / dst_meta)
    }

    pub fn new_meta_rel_path(&self) -> Utf8PathBuf {
        let mut dst_meta = self.containing_folder.clone();
        dst_meta.push(&self.name);
        dst_meta.set_extension("save.meta");
        dst_meta
    }

    fn create_unique_name(assets: &Assets, ext: &str) -> AssetNameBuf {
        loop {
            let mut new_name = AssetNameBuf::from(uuid::Uuid::new_v4().to_string());
            new_name.set_extension(ext);

            if assets.exists(&new_name) {
                continue;
            }

            break new_name;
        }
    }
}

impl EditorTask for SaveSceneTask {
    fn has_confirm_ui(&self) -> bool {
        !self.overwrite
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        if self.overwrite {
            return Ok(TaskConfirmation::Ready);
        }

        ui.text_edit_singleline(&mut self.name);

        let mut valid_name = !self.name.is_empty();
        for c in self.name.chars() {
            if std::path::is_separator(c) {
                valid_name = false;
                break;
            }
        }

        if ui
            .add_enabled(valid_name, util::constructive_button("Save"))
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
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let editor_assets = res.get::<EditorAssets>().unwrap();
        self.assets_root = editor_assets.active_assets_root().into();
        self.package_root = editor_assets.active_package_root().into();

        let asset_exists_already = editor_assets
            .find_asset(&self.new_meta_rel_path())
            .is_some();

        self.overwrite = asset_exists_already;

        let assets = res.get::<Assets>().unwrap().clone();
        let scene_graph = res.get::<SceneGraph>().unwrap();
        let entities = scene_graph.all_entities(queries);
        let (save_data, _) = crate::ser::saver::<Ron>().save(assets.clone(), queries, &entities)?;

        self.save_data = Some(save_data);
        self.assets = Some(assets);

        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let assets = self.assets.as_ref().unwrap();

        let (meta_file, data_path) = if self.overwrite {
            let meta_path = self.new_meta_path();
            let f = std::fs::OpenOptions::new().read(true).open(meta_path)?;
            let r = BufReader::new(f);
            let meta_file = ron::de::from_reader::<_, MetaFile>(r)?;
            let header_path = path!(self.package_root / meta_file.baked);

            let f = std::fs::OpenOptions::new().read(true).open(header_path)?;
            let r = BufReader::new(f);
            let header_file = bincode::deserialize_from::<_, SceneAssetHeader>(r)?;

            (meta_file, header_file.data_path)
        } else {
            let baked = Self::create_unique_name(assets, SceneAsset::EXTENSION);
            let data_path = Self::create_unique_name(assets, "");
            let meta_file = MetaFile {
                baked,
                data: MetaData::Scene,
            };

            (meta_file, data_path)
        };

        let save_data = self.save_data.take().unwrap();

        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path!(self.package_root / data_path))?;
        let w = BufWriter::new(f);
        bincode::serialize_into(w, &save_data)?;
        assets.scan_for(&data_path);

        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path!(self.package_root / meta_file.baked))?;
        let w = BufWriter::new(f);
        bincode::serialize_into(w, &SceneAssetHeader { data_path })?;

        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.new_meta_path())?;
        let w = BufWriter::new(f);
        ron::ser::to_writer(w, &meta_file)?;

        std::fs::write(self.new_raw_path(), &[])?;

        self.meta_file = Some(meta_file);

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let assets = self.assets.take().unwrap();
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();
        let meta_file = self.meta_file.take().unwrap();

        assets.scan_for(&meta_file.baked);
        editor_assets.remove_from_active_package(&self.new_meta_rel_path());
        editor_assets.scan_for(Utf8PathBuf::from_path_buf(self.new_meta_path()).unwrap())?;

        res.get_mut::<SceneGraph>()
            .unwrap()
            .set_active_scene(self.new_meta_rel_path());

        Ok(())
    }
}
