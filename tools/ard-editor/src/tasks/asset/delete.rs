use std::{fs::File, io::BufReader, path::PathBuf};

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
use rustc_hash::FxHashSet;

use crate::assets::meta::AssetType;

pub struct DeleteAssetVisitor {
    package_root: PathBuf,
    visited: FxHashSet<AssetNameBuf>,
}

impl DeleteAssetVisitor {
    pub fn new(package_root: impl Into<PathBuf>) -> Self {
        Self {
            package_root: package_root.into(),
            visited: FxHashSet::default(),
        }
    }

    #[inline(always)]
    pub fn visited(&self) -> &FxHashSet<AssetNameBuf> {
        &self.visited
    }

    pub fn visit(&mut self, asset: &AssetName) -> anyhow::Result<()> {
        if self.visited.contains(asset) {
            return Ok(());
        }

        let header_path = path!(self.package_root / asset);
        let in_file = BufReader::new(File::open(&header_path)?);
        let asset_ty = AssetType::try_from(asset.as_std_path())?;
        self.visited.insert(asset.into());

        match asset_ty {
            AssetType::Model => {
                let header = bincode::deserialize_from::<_, ModelHeader>(in_file)?;
                self.visit_model(header)?;
            }
            AssetType::Mesh => {
                let header = bincode::deserialize_from::<_, MeshHeader>(in_file)?;
                self.visit_mesh(header)?;
            }
            AssetType::Texture => {
                let header = bincode::deserialize_from::<_, TextureHeader>(in_file)?;
                self.visit_texture(header)?;
            }
            AssetType::Material => {
                let header = bincode::deserialize_from::<_, MaterialHeader<AssetNameBuf>>(in_file)?;
                self.visit_material(header)?;
            }
            AssetType::Scene => {
                let header = bincode::deserialize_from::<_, SceneAssetHeader>(in_file)?;
                self.visit_scene(header)?;
            }
        }

        Ok(std::fs::remove_file(header_path)?)
    }

    fn visit_model(&mut self, header: ModelHeader) -> anyhow::Result<()> {
        for texture in &header.textures {
            self.visit(texture)?;
        }

        for mesh in &header.meshes {
            self.visit(mesh)?;
        }

        for material in &header.materials {
            self.visit(material)?;
        }

        Ok(())
    }

    fn visit_texture(&mut self, header: TextureHeader) -> anyhow::Result<()> {
        for mip in &header.mips {
            let src = path!(self.package_root / mip);
            std::fs::remove_file(src)?;
            self.visited.insert(mip.clone());
        }

        Ok(())
    }

    fn visit_mesh(&mut self, header: MeshHeader) -> anyhow::Result<()> {
        let src = path!(self.package_root / header.data_path);
        std::fs::remove_file(src)?;
        self.visited.insert(header.data_path.clone());

        Ok(())
    }

    fn visit_material(&mut self, header: MaterialHeader<AssetNameBuf>) -> anyhow::Result<()> {
        match &header.ty {
            MaterialType::Pbr {
                diffuse_map,
                normal_map,
                metallic_roughness_map,
                ..
            } => {
                if let Some(tex) = diffuse_map {
                    self.visit(&tex)?;
                }

                if let Some(tex) = normal_map {
                    self.visit(&tex)?;
                }

                if let Some(tex) = metallic_roughness_map {
                    self.visit(&tex)?;
                }
            }
        }

        Ok(())
    }

    fn visit_scene(&mut self, header: SceneAssetHeader) -> anyhow::Result<()> {
        let src = path!(self.package_root / header.data_path);
        std::fs::remove_file(src)?;
        self.visited.insert(header.data_path.clone());

        Ok(())
    }
}
