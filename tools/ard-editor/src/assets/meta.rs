use std::path::Path;

use ard_engine::assets::asset::AssetNameBuf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct MetaFile {
    pub baked: AssetNameBuf,
    pub data: MetaData,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum MetaData {
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AssetType {
    Model,
    Mesh,
    Texture,
    Material,
}

impl MetaFile {
    pub const EXTENSION: &'static str = "meta";
}

impl MetaData {
    pub fn ty(&self) -> AssetType {
        match self {
            MetaData::Model => AssetType::Model,
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
            "glb" | "ard_mdl" => Ok(AssetType::Model),
            "ard_msh" => Ok(AssetType::Mesh),
            "ard_tex" => Ok(AssetType::Texture),
            "ard_mat" => Ok(AssetType::Material),
            _ => Err(anyhow::Error::msg("Unknown extension.")),
        }
    }
}
