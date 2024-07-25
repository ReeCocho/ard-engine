use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use ard_engine::{
    assets::prelude::*,
    formats::{
        material::{MaterialHeader, MaterialType},
        mesh::MeshHeader,
        model::ModelHeader,
        texture::TextureHeader,
    },
    game::save_data::SceneAssetHeader,
};
use path_macro::path;
use rustc_hash::FxHashMap;

use crate::assets::meta::AssetType;

pub struct RenameAssetVisitor {
    package_root: PathBuf,
    old_to_new: FxHashMap<AssetNameBuf, AssetNameBuf>,
}

impl RenameAssetVisitor {
    pub fn new(package_root: impl Into<PathBuf>) -> Self {
        Self {
            package_root: package_root.into(),
            old_to_new: FxHashMap::default(),
        }
    }

    pub fn scan(&self, assets: &Assets) {
        self.old_to_new.iter().for_each(|(old, new)| {
            assets.scan_for(old);
            assets.scan_for(new);
        });
    }

    pub fn visit(&mut self, assets: &Assets, asset: &AssetName) -> anyhow::Result<AssetNameBuf> {
        if let Some(name) = self.old_to_new.get(asset) {
            return Ok(name.clone());
        }

        let header_path = path!(self.package_root / asset);
        let in_file = BufReader::new(File::open(&header_path)?);
        let asset_ty = AssetType::try_from(asset.as_std_path())?;

        let new_name = Self::create_unique_name(assets, &asset);
        self.old_to_new.insert(asset.into(), new_name.clone());

        let mut out_file =
            BufWriter::new(std::fs::File::create(path!(self.package_root / new_name))?);

        match asset_ty {
            AssetType::Model => {
                let mut header = bincode::deserialize_from::<_, ModelHeader>(in_file)?;
                self.visit_model(assets, &mut header)?;
                bincode::serialize_into(&mut out_file, &header).unwrap();
            }
            AssetType::Mesh => {
                let mut header = bincode::deserialize_from::<_, MeshHeader>(in_file)?;
                self.visit_mesh(assets, &mut header)?;
                bincode::serialize_into(&mut out_file, &header).unwrap();
            }
            AssetType::Texture => {
                let mut header = bincode::deserialize_from::<_, TextureHeader>(in_file)?;
                self.visit_texture(assets, &mut header)?;
                bincode::serialize_into(&mut out_file, &header).unwrap();
            }
            AssetType::Material => {
                let mut header =
                    bincode::deserialize_from::<_, MaterialHeader<AssetNameBuf>>(in_file)?;
                self.visit_material(assets, &mut header)?;
                bincode::serialize_into(&mut out_file, &header).unwrap();
            }
            AssetType::Scene => {
                let mut header = bincode::deserialize_from::<_, SceneAssetHeader>(in_file)?;
                self.visit_scene(assets, &mut header)?;
                bincode::serialize_into(&mut out_file, &header).unwrap();
            }
        }

        std::fs::remove_file(header_path)?;

        Ok(new_name)
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

    fn visit_model(&mut self, assets: &Assets, header: &mut ModelHeader) -> anyhow::Result<()> {
        for texture in &mut header.textures {
            *texture = self.visit(assets, texture)?;
        }

        for mesh in &mut header.meshes {
            *mesh = self.visit(assets, mesh)?;
        }

        for material in &mut header.materials {
            *material = self.visit(assets, material)?;
        }

        Ok(())
    }

    fn visit_texture(&mut self, assets: &Assets, header: &mut TextureHeader) -> anyhow::Result<()> {
        for mip in &mut header.mips {
            let new_mip_name = Self::create_unique_name(assets, mip);
            let src = path!(self.package_root / mip);
            let dst = path!(self.package_root / new_mip_name);
            std::fs::rename(src, dst)?;
            self.old_to_new.insert(mip.clone(), new_mip_name.clone());
            *mip = new_mip_name;
        }

        Ok(())
    }

    fn visit_mesh(&mut self, assets: &Assets, header: &mut MeshHeader) -> anyhow::Result<()> {
        let new_data_name = Self::create_unique_name(assets, &header.data_path);
        let src = path!(self.package_root / header.data_path);
        let dst = path!(self.package_root / new_data_name);
        std::fs::rename(src, dst)?;

        self.old_to_new
            .insert(header.data_path.clone(), new_data_name.clone());
        header.data_path = new_data_name;

        Ok(())
    }

    fn visit_material(
        &mut self,
        assets: &Assets,
        header: &mut MaterialHeader<AssetNameBuf>,
    ) -> anyhow::Result<()> {
        match &mut header.ty {
            MaterialType::Pbr {
                diffuse_map,
                normal_map,
                metallic_roughness_map,
                ..
            } => {
                if let Some(tex) = diffuse_map {
                    *tex = self.visit(assets, &tex)?;
                }

                if let Some(tex) = normal_map {
                    *tex = self.visit(assets, &tex)?;
                }

                if let Some(tex) = metallic_roughness_map {
                    *tex = self.visit(assets, &tex)?;
                }
            }
        }

        Ok(())
    }

    fn visit_scene(
        &mut self,
        assets: &Assets,
        header: &mut SceneAssetHeader,
    ) -> anyhow::Result<()> {
        let new_data_name = Self::create_unique_name(assets, &header.data_path);
        let src = path!(self.package_root / header.data_path);
        let dst = path!(self.package_root / new_data_name);
        std::fs::rename(src, dst)?;

        self.old_to_new
            .insert(header.data_path.clone(), new_data_name.clone());
        header.data_path = new_data_name;

        Ok(())
    }
}
