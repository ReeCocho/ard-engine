use ard_assets::prelude::*;
use ard_formats::mesh::{MeshData, MeshHeader};
use ard_render::factory::Factory;
use ard_render_meshes::mesh::{Mesh, MeshCreateInfo};
use async_trait::async_trait;

pub struct MeshLoader {
    factory: Factory,
}

pub struct MeshAsset {
    pub mesh: Mesh,
}

impl Asset for MeshAsset {
    const EXTENSION: &'static str = "ard_msh";
    type Loader = MeshLoader;
}

impl MeshLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
    }
}

#[async_trait]
impl AssetLoader for MeshLoader {
    type Asset = MeshAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the mesh header
        let header = package.read(asset.to_owned()).await?;
        let header = match bincode::deserialize::<MeshHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Decode mesh data
        let data = package.read(header.data_path).await?;
        let source = match bincode::deserialize::<MeshData>(&data) {
            Ok(data) => data,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Create mesh
        let create_info = MeshCreateInfo {
            debug_name: Some(format!("{asset:?}")),
            data: source,
        };

        match self.factory.create_mesh(create_info) {
            Ok(mesh) => Ok(AssetLoadResult::Loaded {
                asset: MeshAsset { mesh },
                persistent: false,
            }),
            Err(err) => Err(AssetLoadError::Other(err.to_string())),
        }
    }

    async fn post_load(
        &self,
        _assets: Assets,
        _package: Package,
        _handle: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        Ok(AssetPostLoadResult::Loaded)
    }
}
