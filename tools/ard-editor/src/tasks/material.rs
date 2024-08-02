use std::path::PathBuf;

use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    formats::material::{BlendType, MaterialHeader, MaterialType},
    math::Vec4,
    render::material::MaterialAsset,
};
use camino::Utf8PathBuf;
use path_macro::path;

use crate::{
    assets::{
        meta::{MetaData, MetaFile},
        op::AssetNameGenerator,
        CurrentAssetPath, EditorAssets,
    },
    gui::util,
};

use super::{EditorTask, TaskConfirmation};

#[derive(Default)]
pub struct CreateMaterialTask {
    active_package: PathBuf,
    active_assets: Utf8PathBuf,
    dst_folder: Utf8PathBuf,
    name: String,
    new_asset: AssetNameBuf,
    assets: Option<Assets>,
}

pub struct SaveMaterialTask {
    active_package: PathBuf,
    asset: Handle<MaterialAsset>,
}

impl CreateMaterialTask {
    pub fn meta_file_path_rel(&self) -> Utf8PathBuf {
        let mut name = self.dst_folder.clone();
        name.push(format!("{}.mat.meta", self.name));
        name
    }

    pub fn meta_file_path(&self) -> Utf8PathBuf {
        let mut name = self.active_assets.clone();
        name.push(self.meta_file_path_rel());
        name
    }

    pub fn raw_file_path(&self) -> Utf8PathBuf {
        let mut name = self.active_assets.clone();
        name.push(&self.dst_folder);
        name.push(format!("{}.mat", self.name));
        name
    }

    pub fn baked_file_path(&self) -> PathBuf {
        path!(self.active_package / self.new_asset)
    }
}

impl SaveMaterialTask {
    pub fn new(asset: Handle<MaterialAsset>) -> Self {
        Self {
            active_package: PathBuf::default(),
            asset,
        }
    }
}

impl EditorTask for CreateMaterialTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        ui.text_edit_singleline(&mut self.name);

        let mut valid_name = !self.name.is_empty();
        for c in self.name.chars() {
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
    ) -> anyhow::Result<()> {
        let editor_assets = res.get::<EditorAssets>().unwrap();

        self.active_package = editor_assets.active_package_root().into();
        self.active_assets = editor_assets.active_assets_root().into();
        self.dst_folder = res.get::<CurrentAssetPath>().unwrap().path().into();
        self.assets = Some(res.get::<Assets>().unwrap().clone());

        if editor_assets
            .find_asset(self.meta_file_path_rel())
            .is_some()
        {
            return Err(anyhow::Error::msg("Asset with that name already exists."));
        }

        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let header = MaterialHeader::<AssetNameBuf> {
            blend_ty: BlendType::Opaque,
            ty: MaterialType::Pbr {
                base_color: Vec4::ONE,
                metallic: 0.0,
                roughness: 1.0,
                alpha_cutoff: 0.0,
                diffuse_map: None,
                normal_map: None,
                metallic_roughness_map: None,
            },
        };

        let assets = self.assets.as_ref().unwrap();
        let mut gen = AssetNameGenerator::new(assets.clone());
        self.new_asset = gen.generate(MaterialAsset::EXTENSION);

        let file = std::fs::File::create(self.baked_file_path())?;
        let writer = std::io::BufWriter::new(file);
        bincode::serialize_into(writer, &header)?;

        let file = std::fs::File::create(self.meta_file_path())?;
        let writer = std::io::BufWriter::new(file);
        ron::ser::to_writer(
            writer,
            &MetaFile {
                baked: self.new_asset.clone(),
                data: MetaData::Material,
            },
        )?;

        std::fs::write(self.raw_file_path(), &[])?;

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        self.assets.take().unwrap().scan_for(&self.new_asset);
        res.get_mut::<EditorAssets>()
            .unwrap()
            .scan_for(self.meta_file_path())
            .unwrap();
        Ok(())
    }
}

impl EditorTask for SaveMaterialTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        Ok(TaskConfirmation::Ready)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        self.active_package = res
            .get::<EditorAssets>()
            .unwrap()
            .active_package_root()
            .into();
        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let assets = self.asset.assets();
        let asset_name = assets.get_name(&self.asset);
        let path = path!(self.active_package / asset_name);
        let mat = match assets.get(&self.asset) {
            Some(mat) => mat,
            None => return Ok(()),
        };
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        Ok(bincode::serialize_into(writer, &mat.header)?)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
