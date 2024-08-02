use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::Result;
use ard_engine::{
    assets::prelude::*,
    formats::{
        material::{MaterialHeader, MaterialType},
        mesh::MeshHeader,
        model::ModelHeader,
        texture::TextureHeader,
    },
    game::save_data::SceneAssetHeader,
    log::info,
};
use enum_dispatch::enum_dispatch;
use path_macro::path;
use rustc_hash::{FxHashMap, FxHashSet};

use super::meta::AssetType;

#[enum_dispatch]
pub enum AssetHeader {
    ModelHeader,
    MeshHeader,
    TextureHeader,
    MaterialHeader(MaterialHeader<AssetNameBuf>),
    SceneAssetHeader,
}

#[enum_dispatch(AssetHeader)]
pub trait AssetOps {
    fn rename(&mut self, asset_name: &AssetName, ctx: &mut RenameContext) -> Result<AssetNameBuf> {
        self.visit_data(|data_path| {
            let new_name = ctx.gen.generate(data_path.extension().unwrap_or(""));
            let src = path!(ctx.package_root / data_path);
            let dst = path!(ctx.package_root / new_name);
            std::fs::rename(src, dst)?;
            ctx.old_to_new.insert(data_path.clone(), new_name.clone());
            *data_path = new_name;
            Ok(())
        })?;

        self.visit_sub_assets(|asset_path| {
            // Check if we've already seen this asset before
            if let Some(name) = ctx.old_to_new.get(asset_path) {
                *asset_path = name.clone();
                return Ok(());
            }

            let header_path = path!(ctx.package_root / asset_path);
            let mut header = AssetHeader::load(&header_path)?;
            let new_name = header.rename(asset_path, ctx)?;
            *asset_path = new_name;
            header.save(path!(ctx.package_root / asset_path))?;

            Ok(())
        })?;

        let new_name = ctx.gen.generate(asset_name.extension().unwrap_or(""));
        let src_path = path!(ctx.package_root / asset_name);
        let dst_path = path!(ctx.package_root / new_name);
        ctx.old_to_new
            .insert(asset_name.to_owned(), new_name.clone());
        std::fs::rename(src_path, dst_path)?;

        Ok(new_name)
    }

    fn delete(&mut self, asset_name: &AssetName, ctx: &mut DeleteContext) -> Result<()> {
        self.visit_data(|data_path| {
            let abs_path = path!(ctx.package_root / data_path);
            std::fs::remove_file(abs_path)?;
            ctx.visited.insert(data_path.clone());
            Ok(())
        })?;

        self.visit_sub_assets(|asset_path| {
            // Check if we've already seen this asset before
            if ctx.visited.contains(asset_path) {
                return Ok(());
            }

            let header_path = path!(ctx.package_root / asset_path);
            let mut header = AssetHeader::load(&header_path)?;
            header.delete(asset_path, ctx)?;

            Ok(())
        })?;

        ctx.visited.insert(asset_name.to_owned());
        std::fs::remove_file(path!(ctx.package_root / asset_name))?;

        Ok(())
    }

    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()>;

    fn visit_data(&mut self, func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()>;

    fn visit_sub_assets(&mut self, func: impl FnMut(&mut AssetNameBuf) -> Result<()>)
        -> Result<()>;
}

pub struct RenameContext {
    pub package_root: PathBuf,
    pub old_to_new: FxHashMap<AssetNameBuf, AssetNameBuf>,
    pub gen: AssetNameGenerator,
}

pub struct AssetNameGenerator {
    assets: Assets,
    new_names: FxHashSet<AssetNameBuf>,
}

pub struct DeleteContext {
    pub package_root: PathBuf,
    pub visited: FxHashSet<AssetNameBuf>,
}

pub struct SaveContext {
    pub package_root: PathBuf,
}

impl AssetHeader {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path: PathBuf = path.into();
        let ty = AssetType::try_from(path.as_path())?;
        let reader = BufReader::new(File::open(path)?);
        match ty {
            AssetType::Model => Ok(bincode::deserialize_from::<_, ModelHeader>(reader)?.into()),
            AssetType::Mesh => Ok(bincode::deserialize_from::<_, MeshHeader>(reader)?.into()),
            AssetType::Texture => Ok(bincode::deserialize_from::<_, TextureHeader>(reader)?.into()),
            AssetType::Material => {
                Ok(bincode::deserialize_from::<_, MaterialHeader<AssetNameBuf>>(reader)?.into())
            }
            AssetType::Scene => {
                Ok(bincode::deserialize_from::<_, SceneAssetHeader>(reader)?.into())
            }
        }
    }

    fn save(self, path: impl Into<PathBuf>) -> Result<()> {
        let path: PathBuf = path.into();
        let writer = BufWriter::new(File::create(path)?);
        Ok(match self {
            AssetHeader::ModelHeader(header) => bincode::serialize_into(writer, &header)?,
            AssetHeader::MeshHeader(header) => bincode::serialize_into(writer, &header)?,
            AssetHeader::TextureHeader(header) => bincode::serialize_into(writer, &header)?,
            AssetHeader::MaterialHeader(header) => bincode::serialize_into(writer, &header)?,
            AssetHeader::SceneAssetHeader(header) => bincode::serialize_into(writer, &header)?,
        })
    }
}

impl AssetNameGenerator {
    pub fn new(assets: Assets) -> Self {
        Self {
            assets,
            new_names: FxHashSet::default(),
        }
    }

    #[inline(always)]
    pub fn assets(&self) -> &Assets {
        &self.assets
    }

    #[inline(always)]
    pub fn new_names(&self) -> &FxHashSet<AssetNameBuf> {
        &self.new_names
    }

    pub fn generate(&mut self, ext: impl AsRef<AssetName>) -> AssetNameBuf {
        loop {
            let mut new_name = AssetNameBuf::from(uuid::Uuid::new_v4().to_string());
            new_name.set_extension(ext.as_ref());
            if self.assets.exists(&new_name) || self.new_names.contains(&new_name) {
                info!(
                    "Congrats! You encountered a UUID name collision! \
                    Don't worry, this isn't an error. It's just astronomically \
                    rare. You should be proud."
                );
                continue;
            }
            self.new_names.insert(new_name.clone());
            break new_name;
        }
    }
}

impl AssetOps for ModelHeader {
    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()> {
        let out_path = path!(ctx.package_root / asset_name);
        let file = File::create(out_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    fn visit_data(&mut self, _func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()> {
        Ok(())
    }

    fn visit_sub_assets(
        &mut self,
        mut func: impl FnMut(&mut AssetNameBuf) -> Result<()>,
    ) -> Result<()> {
        for mat in &mut self.materials {
            func(mat)?;
        }

        for mesh in &mut self.meshes {
            func(mesh)?;
        }

        for tex in &mut self.textures {
            func(tex)?;
        }

        Ok(())
    }
}

impl AssetOps for MeshHeader {
    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()> {
        let out_path = path!(ctx.package_root / asset_name);
        let file = File::create(out_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    fn visit_data(&mut self, mut func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()> {
        func(&mut self.data_path)
    }

    fn visit_sub_assets(
        &mut self,
        _func: impl FnMut(&mut AssetNameBuf) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }
}

impl AssetOps for MaterialHeader<AssetNameBuf> {
    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()> {
        let out_path = path!(ctx.package_root / asset_name);
        let file = File::create(out_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    fn visit_data(&mut self, _func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()> {
        Ok(())
    }

    fn visit_sub_assets(
        &mut self,
        mut func: impl FnMut(&mut AssetNameBuf) -> Result<()>,
    ) -> Result<()> {
        match &mut self.ty {
            MaterialType::Pbr {
                diffuse_map,
                normal_map,
                metallic_roughness_map,
                ..
            } => {
                if let Some(tex) = diffuse_map {
                    func(tex)?;
                }
                if let Some(tex) = normal_map {
                    func(tex)?;
                }
                if let Some(tex) = metallic_roughness_map {
                    func(tex)?;
                }
            }
        }
        Ok(())
    }
}

impl AssetOps for TextureHeader {
    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()> {
        let out_path = path!(ctx.package_root / asset_name);
        let file = File::create(out_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    fn visit_data(&mut self, mut func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()> {
        for mip in &mut self.mips {
            func(mip)?;
        }
        Ok(())
    }

    fn visit_sub_assets(
        &mut self,
        _func: impl FnMut(&mut AssetNameBuf) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }
}

impl AssetOps for SceneAssetHeader {
    fn save(&mut self, asset_name: &AssetName, ctx: &mut SaveContext) -> Result<()> {
        let out_path = path!(ctx.package_root / asset_name);
        let file = File::create(out_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    fn visit_data(&mut self, mut func: impl FnMut(&mut AssetNameBuf) -> Result<()>) -> Result<()> {
        func(&mut self.data_path)
    }

    fn visit_sub_assets(
        &mut self,
        _func: impl FnMut(&mut AssetNameBuf) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }
}
