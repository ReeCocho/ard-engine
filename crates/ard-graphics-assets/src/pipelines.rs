use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::prelude as graphics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::shaders::ShaderAsset;

/// A pipeline that can be loaded from disk.
pub struct PipelineAsset {
    /// The pipeline handle.
    pub pipeline: graphics::Pipeline,
    /// Handle to the vertex shader.
    vertex: Handle<ShaderAsset>,
    /// Handle to the fragment shader.
    fragment: Handle<ShaderAsset>,
}

pub struct PipelineLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a pipeline.
#[derive(Debug, Serialize, Deserialize)]
struct PipelineMeta {
    /// Name of the vertex shader.
    pub vertex: AssetNameBuf,
    /// Name of the fragment shader.
    pub fragment: AssetNameBuf,
}

impl Asset for PipelineAsset {
    const EXTENSION: &'static str = "pip";

    type Loader = PipelineLoader;
}

#[async_trait]
impl AssetLoader for PipelineLoader {
    type Asset = PipelineAsset;

    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;

        let meta = match ron::from_str::<PipelineMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Load both of the shaders
        let vertex = assets.load_async::<ShaderAsset>(&meta.vertex).await;
        let fragment = assets.load_async::<ShaderAsset>(&meta.fragment).await;

        // Create the pipeline
        let create_info = PipelineCreateInfo {
            vertex: assets.get(&vertex).unwrap().shader.clone(),
            fragment: assets.get(&fragment).unwrap().shader.clone(),
        };

        let pipeline = self.factory.create_pipeline(&create_info);

        Ok(AssetLoadResult::Loaded {
            asset: PipelineAsset {
                pipeline,
                vertex,
                fragment,
            },
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
