use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::{camera::Factory, prelude as graphics};
use ard_math::Vec4;
use async_trait::async_trait;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Physically-based-rendering materials must use a pipeline with the following properties.
///
/// # Vertex layout
/// The vertex layout must have positions, normals, and uv0.
///
/// # UBO
/// The UBO size must be equal to the size of `PbrMaterialData`.
///
/// # Textures
/// The texture at slot 0 is for the base color.
/// The texture at slot 1 is for the metallic and roughness.
///
pub struct PbrMaterialAsset {
    /// Handle to the material object.
    pub material: graphics::Material,
    /// Actual PBR material data.
    data: PbrMaterialData,
}

/// Data sent to the GPU that represents the PBR materials data.
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct PbrMaterialData {
    pub base_color: Vec4,
    pub metallic: f32,
    pub roughness: f32,
}

unsafe impl Pod for PbrMaterialData {}
unsafe impl Zeroable for PbrMaterialData {}

pub struct PbrMaterialLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a PBR material.
#[derive(Debug, Serialize, Deserialize)]
struct PbrMaterialMeta {
    /// Name of the pipeline to use for the material.
    pub pipeline: AssetNameBuf,
    /// Actual PBR material data.
    pub data: PbrMaterialData,
}

impl PbrMaterialAsset {
    #[inline]
    pub fn data(&self) -> &PbrMaterialData {
        &self.data
    }

    #[inline]
    pub fn set_data(&mut self, factory: &Factory, data: PbrMaterialData) {
        factory.update_material_data(&self.material, &bytemuck::bytes_of(&data));
    }
}

impl Asset for PbrMaterialAsset {
    const EXTENSION: &'static str = "pbr";

    type Loader = PbrMaterialLoader;
}

#[async_trait]
impl AssetLoader for PbrMaterialLoader {
    type Asset = PbrMaterialAsset;

    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the meta file
        let meta = package.read_str(asset).await?;

        let meta = match ron::from_str::<PbrMaterialMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Load the pipeline
        let pipeline = assets
            .load_async::<crate::PipelineAsset>(&meta.pipeline)
            .await;

        // Create the material
        let create_info = graphics::MaterialCreateInfo {
            pipeline: assets.get(&pipeline).unwrap().pipeline.clone(),
        };

        let material = self.factory.create_material(&create_info);

        // Upload material data
        self.factory
            .update_material_data(&material, bytemuck::bytes_of(&meta.data));

        Ok(AssetLoadResult::Loaded {
            asset: PbrMaterialAsset {
                material,
                data: meta.data,
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
