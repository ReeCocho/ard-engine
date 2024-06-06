use std::path::{Path, PathBuf};

use ard_engine::assets::asset::AssetNameBuf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct MetaFile {
    pub raw: PathBuf,
    pub baked: AssetNameBuf,
    pub data: MetaData,
}

#[derive(Serialize, Deserialize)]
pub enum MetaData {
    Model,
}

pub enum AssetType {
    Model,
}

impl MetaFile {
    pub const EXTENSION: &'static str = "meta";
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
