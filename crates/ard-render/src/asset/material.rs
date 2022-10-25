use ard_assets::prelude::*;
use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{CullMode, FrontFace, ShaderCreateInfo};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    factory::Factory,
    material::{Material, MaterialCreateInfo},
};

pub struct MaterialAsset {
    pub material: Material,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MaterialDescriptor {
    pub vertex_shader_path: AssetNameBuf,
    pub depth_only_shader_path: Option<AssetNameBuf>,
    pub fragment_shader_path: AssetNameBuf,
    pub vertex_layout: MaterialVertexLayout,
    pub material_data_size: u64,
    pub texture_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct MaterialVertexLayout {
    pub normals: bool,
    pub tangents: bool,
    pub colors: bool,
    pub uv0: bool,
    pub uv1: bool,
    pub uv2: bool,
    pub uv3: bool,
}

pub struct MaterialLoader {
    factory: Factory,
}

impl Asset for MaterialAsset {
    const EXTENSION: &'static str = "mat";
    type Loader = MaterialLoader;
}

impl MaterialLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
    }
}

#[async_trait]
impl AssetLoader for MaterialLoader {
    type Asset = MaterialAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the descriptor
        let desc = package.read_str(asset).await?;
        let desc = match ron::from_str::<MaterialDescriptor>(&desc) {
            Ok(desc) => desc,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Read in the shader data
        let vertex_shd = package.read(&desc.vertex_shader_path).await?;
        let fragment_shd = package.read(&desc.fragment_shader_path).await?;

        // Create shaders
        let vertex_shd = match self.factory.create_shader(ShaderCreateInfo {
            code: &vertex_shd,
            debug_name: Some(desc.vertex_shader_path.to_string_lossy().to_string()),
        }) {
            Ok(shd) => shd,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        let depth_only_shd = match &desc.depth_only_shader_path {
            Some(path) => {
                let shd = package.read(path).await?;
                match self.factory.create_shader(ShaderCreateInfo {
                    code: &shd,
                    debug_name: Some(path.to_string_lossy().to_string()),
                }) {
                    Ok(shd) => Some(shd),
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                }
            }
            None => None,
        };

        let fragment_shd = match self.factory.create_shader(ShaderCreateInfo {
            code: &fragment_shd,
            debug_name: Some(desc.fragment_shader_path.to_string_lossy().to_string()),
        }) {
            Ok(shd) => shd,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Create material
        let material = self.factory.create_material(MaterialCreateInfo {
            vertex_shader: vertex_shd,
            depth_only_shader: depth_only_shd,
            fragment_shader: fragment_shd,
            vertex_layout: desc.vertex_layout.into(),
            texture_count: desc.texture_count,
            data_size: desc.material_data_size,
            cull_mode: CullMode::Back,
            front_face: FrontFace::Clockwise,
        });

        Ok(AssetLoadResult::Loaded {
            asset: MaterialAsset { material },
            persistent: true,
        })
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}

impl From<MaterialVertexLayout> for VertexLayout {
    #[inline(always)]
    fn from(layout: MaterialVertexLayout) -> Self {
        let mut out = VertexLayout::empty();
        if layout.normals {
            out |= VertexLayout::NORMAL;
        }
        if layout.tangents {
            out |= VertexLayout::TANGENT;
        }
        if layout.colors {
            out |= VertexLayout::COLOR;
        }
        if layout.uv0 {
            out |= VertexLayout::UV0;
        }
        if layout.uv1 {
            out |= VertexLayout::UV1;
        }
        if layout.uv2 {
            out |= VertexLayout::UV2;
        }
        if layout.uv3 {
            out |= VertexLayout::UV3;
        }
        out
    }
}
