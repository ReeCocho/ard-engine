use std::path::Path;

use ard_engine::{
    assets::asset::{Asset, AssetNameBuf},
    game::save_data::SceneAsset,
    render::{material::MaterialAsset, mesh::MeshAsset, model::ModelAsset, texture::TextureAsset},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct MetaFile {
    pub baked: AssetNameBuf,
    pub data: MetaData,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum MetaData {
    Model,
    Scene,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AssetType {
    Model,
    Mesh,
    Texture,
    Material,
    Scene,
}

impl MetaFile {
    pub const EXTENSION: &'static str = "meta";
}

impl MetaData {
    pub fn ty(&self) -> AssetType {
        match self {
            MetaData::Model => AssetType::Model,
            MetaData::Scene => AssetType::Scene,
        }
    }
}

impl<'a> TryFrom<&'a Path> for AssetType {
    type Error = anyhow::Error;

    fn try_from(value: &'a Path) -> Result<Self, Self::Error> {
        let ext = match value.extension() {
            Some(ext) => ext,
            None => return Err(anyhow::Error::msg("Path has no extension.")),
        };

        let ext = match ext.to_str() {
            Some(ext) => ext,
            None => return Err(anyhow::Error::msg("Path extension invalid.")),
        };

        match ext {
            "glb" | ModelAsset::EXTENSION => Ok(AssetType::Model),
            MeshAsset::EXTENSION => Ok(AssetType::Mesh),
            TextureAsset::EXTENSION => Ok(AssetType::Texture),
            MaterialAsset::EXTENSION => Ok(AssetType::Material),
            SceneAsset::EXTENSION => Ok(AssetType::Scene),
            _ => Err(anyhow::Error::msg("Unknown extension.")),
        }
    }
}
