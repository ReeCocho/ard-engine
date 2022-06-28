use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A shader that can be loaded from disk.
pub struct ShaderAsset {
    /// The shader handle.
    pub shader: graphics::Shader,
}

pub struct ShaderLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a shader.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShaderDescriptor {
    /// Path to the actual shader file. Relative to the package.
    pub file: AssetNameBuf,
    pub ty: ShaderType,
    pub vertex_layout: VertexLayout,
    pub inputs: ShaderInputs,
}

impl Asset for ShaderAsset {
    const EXTENSION: &'static str = "shd";

    type Loader = ShaderLoader;
}

#[async_trait]
impl AssetLoader for ShaderLoader {
    type Asset = ShaderAsset;

    async fn load(
        &self,
        _: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;
        let meta = match ron::from_str::<ShaderDescriptor>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Read in the shader source code
        let data = package.read(&meta.file).await?;

        // Create the shader
        let create_info = graphics::ShaderCreateInfo {
            ty: meta.ty,
            vertex_layout: meta.vertex_layout,
            inputs: meta.inputs,
            code: &data,
        };

        let shader = self.factory.create_shader(&create_info);

        Ok(AssetLoadResult::Loaded {
            asset: ShaderAsset { shader },
            persistent: false,
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
