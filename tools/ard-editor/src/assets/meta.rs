use std::path::Path;

use ard_engine::{
    assets::asset::{Asset, AssetNameBuf},
    formats::texture::Sampler,
    game::save_data::SceneAsset,
    render::{
        material::MaterialAsset,
        mesh::MeshAsset,
        model::ModelAsset,
        prelude::{Filter, SamplerAddressMode},
        texture::TextureAsset,
    },
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
    Material,
    Scene,
    Texture(TextureImportSettings),
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct TextureImportSettings {
    pub sampler: Sampler,
    pub linear_color_space: bool,
    pub compress: bool,
    pub mip: TextureMipSetting,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum TextureMipSetting {
    None,
    GenerateAll,
    GenerateExact(usize),
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
            MetaData::Material => AssetType::Material,
            MetaData::Texture { .. } => AssetType::Texture,
        }
    }
}

impl Default for TextureImportSettings {
    fn default() -> Self {
        TextureImportSettings {
            linear_color_space: false,
            mip: TextureMipSetting::GenerateAll,
            compress: true,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
                address_u: SamplerAddressMode::Repeat,
                address_v: SamplerAddressMode::Repeat,
                anisotropy: true,
            },
        }
    }
}

impl TextureMipSetting {
    pub fn label(&self) -> &'static str {
        match self {
            TextureMipSetting::None => "None",
            TextureMipSetting::GenerateAll => "Generate All",
            TextureMipSetting::GenerateExact(_) => "Generate Exact",
        }
    }
}

impl PartialEq for TextureMipSetting {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TextureMipSetting::None, TextureMipSetting::None) => true,
            (TextureMipSetting::GenerateAll, TextureMipSetting::GenerateAll) => true,
            (TextureMipSetting::GenerateExact(_), TextureMipSetting::GenerateExact(_)) => true,
            _ => false,
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

        match ext.to_lowercase().as_str() {
            "glb" | ModelAsset::EXTENSION => Ok(AssetType::Model),
            MeshAsset::EXTENSION => Ok(AssetType::Mesh),
            "jpg"
            | "jpeg"
            | "png"
            | "bmp"
            | "dds"
            | "tga"
            | "tiff"
            | "webp"
            | TextureAsset::EXTENSION => Ok(AssetType::Texture),
            MaterialAsset::EXTENSION => Ok(AssetType::Material),
            SceneAsset::EXTENSION => Ok(AssetType::Scene),
            _ => Err(anyhow::Error::msg("Unknown extension.")),
        }
    }
}
