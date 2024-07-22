use ard_assets::prelude::*;
use ard_formats::model::{MeshGroup, ModelHeader};
use ard_math::Mat4;
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_render_objects::RenderFlags;
use ard_transform::Model;
use async_trait::async_trait;

use crate::{material::MaterialAsset, mesh::MeshAsset, texture::TextureAsset};

pub struct ModelLoader;

pub struct ModelAsset {
    // pub lights: Vec<Light>,
    pub textures: Vec<Handle<TextureAsset>>,
    pub materials: Vec<Handle<MaterialAsset>>,
    pub mesh_groups: Vec<MeshGroup>,
    pub meshes: Vec<Handle<MeshAsset>>,
    pub node_count: usize,
    pub roots: Vec<Node>,
}

pub struct ModelAssetInstance {
    pub meshes: ModelAssetInstanceMeshes,
}

pub struct ModelAssetInstanceMeshes {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<MaterialInstance>,
    pub models: Vec<Model>,
    pub rendering_mode: Vec<RenderingMode>,
    pub flags: Vec<RenderFlags>,
}

// pub enum Light {
//     Point(PointLight),
//     Spot,
//     Directional,
// }

pub struct Node {
    /// The name of this node.
    pub name: String,
    /// Model matrix for this node in local space.
    pub model: Model,
    /// Data contained within this node.
    pub data: NodeData,
    /// All child nodes of this node.
    pub children: Vec<Node>,
}

pub enum NodeData {
    /// Empty object.
    Empty,
    /// Index of a mesh group.
    MeshGroup(usize),
    // Light(usize),
}

impl Asset for ModelAsset {
    const EXTENSION: &'static str = "ard_mdl";
    type Loader = ModelLoader;
}

#[async_trait]
impl AssetLoader for ModelLoader {
    type Asset = ModelAsset;

    async fn load(
        &self,
        assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the model header
        let header = package.read(asset.to_owned()).await?;
        let header = match bincode::deserialize::<ModelHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Read in textures, meshes, and materials

        // We choose to load in textures first so that they get processed by the factory first.
        // If we don't do this, we might end up seeing error textures while they load
        let meshes = futures::future::join_all(header.meshes.into_iter().map(|path| {
            let assets = assets.clone();
            async move {
                let res = assets.load_async::<MeshAsset>(&path).await;
                match res {
                    Some(handle) => Ok(handle),
                    None => Err(AssetLoadError::Other(format!(
                        "could not load mesh {path:?}"
                    ))),
                }
            }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        let materials = futures::future::join_all(header.materials.into_iter().map(|path| {
            let assets = assets.clone();
            async move {
                let res = assets.load_async::<MaterialAsset>(&path).await;
                match res {
                    Some(handle) => Ok(handle),
                    None => Err(AssetLoadError::Other(format!(
                        "could not load material {path:?}"
                    ))),
                }
            }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        let textures = futures::future::join_all(header.textures.into_iter().map(|path| {
            let assets = assets.clone();
            async move {
                let res = assets.load_async::<TextureAsset>(&path).await;
                match res {
                    Some(handle) => Ok(handle),
                    None => Err(AssetLoadError::Other(format!(
                        "could not load texture {path:?}"
                    ))),
                }
            }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        // Begin constructing the asset
        let mut model = ModelAsset {
            // lights: Vec::default(),
            textures,
            materials,
            mesh_groups: header.mesh_groups.clone(),
            meshes,
            roots: Vec::with_capacity(header.roots.len()),
            node_count: 0,
        };

        // Helper function to parse model nodes recursively
        fn parse_node(node: ard_formats::model::Node) -> (Node, usize) {
            let mut node_count = 1;

            let mut out_node = Node {
                name: node.name,
                model: Model(node.model),
                children: Vec::with_capacity(node.children.len()),
                data: match node.data {
                    ard_formats::model::NodeData::Empty => NodeData::Empty,
                    ard_formats::model::NodeData::MeshGroup(id) => NodeData::MeshGroup(id as usize),
                    ard_formats::model::NodeData::Light(_) => NodeData::Empty, // NodeData::Light(id as usize),
                },
            };

            for child in node.children {
                let (node, child_count) = parse_node(child);
                node_count += child_count;
                out_node.children.push(node);
            }

            (out_node, node_count)
        }

        for root in header.roots {
            let (node, node_count) = parse_node(root);
            model.node_count += node_count;
            model.roots.push(node);
        }

        Ok(AssetLoadResult::Loaded {
            asset: model,
            persistent: false,
        })
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

impl ModelAsset {
    pub fn instantiate(&self, assets: &Assets) -> ModelAssetInstance {
        let mut meshes = ModelAssetInstanceMeshes {
            meshes: Vec::default(),
            materials: Vec::default(),
            models: Vec::default(),
            rendering_mode: Vec::default(),
            flags: Vec::default(),
        };

        fn traverse(
            parent_model: Mat4,
            node: &Node,
            asset: &ModelAsset,
            meshes: &mut ModelAssetInstanceMeshes,
            assets: &Assets,
        ) {
            match &node.data {
                NodeData::Empty => {}
                NodeData::MeshGroup(mesh_group) => {
                    let mesh_group = &asset.mesh_groups[*mesh_group];
                    for instance in &mesh_group.0 {
                        let material = &asset.materials[instance.material as usize];
                        let mesh = &asset.meshes[instance.mesh as usize];
                        let material_asset = assets.get(material).unwrap();
                        meshes.meshes.push(assets.get(mesh).unwrap().mesh.clone());
                        meshes.materials.push(material_asset.instance.clone());
                        meshes.models.push(Model(parent_model * node.model.0));
                        meshes.flags.push(RenderFlags::SHADOW_CASTER);
                        meshes.rendering_mode.push(material_asset.render_mode);
                    }
                }
            }

            for child in &node.children {
                traverse(node.model.0, child, asset, meshes, assets);
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut meshes, assets);
        }

        ModelAssetInstance { meshes }
    }
}
