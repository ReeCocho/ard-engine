use ard_engine::assets::prelude::AssetNameBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub enum AssetType {
    Model,
    Texture,
    Mesh,
    Material,
    Unknown,
}

#[derive(Serialize, Deserialize)]
pub enum AssetMetaData {
    Model {
        /// Path to the raw model asset (glb).
        raw: AssetNameBuf,
        /// Path to the baked `ard_mdl` asset file.
        baked: AssetNameBuf,
        /// Should the textures of this model be compressed?
        compress_textures: bool,
        /// Should the tangents of the meshes in this model be computed based on UVs (otherwise,
        /// they are imported or left empty if non-existant).
        compute_tangents: bool,
    },
    Texture {
        /// Path to the raw texture asset (png/jpg/etc).
        raw: AssetNameBuf,
        /// Path to the baked `ard_tex` asset file.
        baked: AssetNameBuf,
    },
    Mesh {
        /// Path to the raw mesh asset.
        raw: AssetNameBuf,
        /// Path to the baked `ard_msh` asset file.
        baked: AssetNameBuf,
    },
    Material {
        /// Path to the `ard_mat` asset file.
        path: AssetNameBuf,
    },
    Unknown,
}

impl AssetType {
    #[inline]
    pub fn from_ext(ext: &str) -> AssetType {
        match ext.to_lowercase().as_str() {
            "png" | "jpeg" | "jpg" => AssetType::Texture,
            "glb" => AssetType::Model,
            _ => AssetType::Unknown,
        }
    }
}
