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

pub enum AssetType {
    Model,
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
            "glb" => Ok(AssetType::Model),
            _ => Err(anyhow::Error::msg("Unknown extension.")),
        }
    }
}
