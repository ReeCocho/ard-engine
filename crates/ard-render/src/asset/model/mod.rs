use std::path::PathBuf;

use ard_assets::prelude::*;
use ard_ecs::prelude::{Entity, EntityCommands};
use ard_math::Mat4;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    factory::Factory,
    lighting::PointLight,
    material::{Material, MaterialInstance},
    mesh::Mesh,
    renderer::{Model, RenderLayer, Renderable},
    static_geometry::{StaticGeometry, StaticRenderable, StaticRenderableHandle},
    texture::Texture,
};

pub mod gltf;
pub mod mdl;

pub struct ModelAsset {
    pub lights: Vec<Light>,
    pub textures: Vec<Texture>,
    pub materials: Vec<MaterialInstance>,
    pub mesh_groups: Vec<MeshGroup>,
    pub node_count: usize,
    pub roots: Vec<Node>,
    pub(self) post_load: Option<ModelPostLoad>,
}

/// Information required for post load operations on a model.
pub(self) enum ModelPostLoad {
    Gltf,
    Ard {
        /// Path to the textures folder which holds each textures mips.
        texture_path: PathBuf,
        /// First element is the index of the texture. The second element is the mip level that
        /// needs to be loaded next. Elements are sorted largest to smallest by the mip level so
        /// they can be popped when all mips are loaded.
        texture_mips: Vec<(usize, u32)>,
    },
}

pub enum Light {
    Point(PointLight),
    Spot,
    Directional,
}

pub struct MeshGroup(pub Vec<MeshInstance>);

pub struct MeshInstance {
    pub mesh: Mesh,
    pub material: usize,
}

pub struct Node {
    /// The name of this node.
    pub name: String,
    /// Model matrix for this node in local space.
    pub model: Mat4,
    /// Data contained within this node.
    pub data: NodeData,
    /// All child nodes of this node.
    pub children: Vec<Node>,
}

pub enum NodeData {
    Empty,
    MeshGroup(usize),
    Light(usize),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Path to the model file.
    pub model: AssetNameBuf,
    /// Type of model held in the file.
    pub ty: ModelType,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ModelType {
    Gltf,
    Ard,
}

pub struct ModelLoader {
    pub(self) factory: Factory,
    pub(self) pbr_material: Material,
}

impl Asset for ModelAsset {
    const EXTENSION: &'static str = "model";
    type Loader = ModelLoader;
}

impl ModelLoader {
    pub fn new(factory: Factory, pbr_material: Material) -> Self {
        Self {
            factory,
            pbr_material,
        }
    }
}

#[async_trait]
impl AssetLoader for ModelLoader {
    type Asset = ModelAsset;

    async fn load(
        &self,
        _assets: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Read in the descriptor
        let desc = package.read_str(asset).await?;
        let desc = match ron::from_str::<ModelDescriptor>(&desc) {
            Ok(desc) => desc,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        let asset = match desc.ty {
            ModelType::Gltf => {
                let data = package.read(&desc.model).await?;
                let gltf = match ard_gltf::GltfModel::from_slice(&data) {
                    Ok(gltf) => gltf,
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                };
                gltf::to_asset(gltf, self)
            }
            ModelType::Ard => mdl::to_asset(&desc.model, &package, self).await?,
        };

        Ok(AssetLoadResult::NeedsPostLoad {
            asset,
            persistent: false,
        })
    }

    async fn post_load(
        &self,
        assets: Assets,
        package: Package,
        handle: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        // Grab the post load information
        let post_load_info = assets.get_mut(&handle).unwrap().post_load.take().unwrap();

        match post_load_info {
            ModelPostLoad::Gltf => {}
            ModelPostLoad::Ard {
                texture_path,
                mut texture_mips,
            } => {
                while !texture_mips.is_empty() {
                    for i in (0..texture_mips.len()).rev() {
                        let (texture, next_mip) = texture_mips[i];

                        // Load mip
                        let mut path = texture_path.clone();
                        path.push(format!("{texture}"));
                        path.push(format!("{next_mip}"));
                        let data = package.read(&path).await?;

                        // Update mip
                        self.factory.load_texture_mip(
                            &assets.get(&handle).unwrap().textures[texture],
                            next_mip as usize,
                            &data,
                        );

                        // If this is the last mip, the texture is now loaded
                        if next_mip == 0 {
                            texture_mips.pop();
                        }
                        // Otherwise, we move to the next mip
                        else {
                            texture_mips[i].1 -= 1;
                        }
                    }
                }
            }
        }

        Ok(AssetPostLoadResult::Loaded)
    }
}

impl ModelAsset {
    pub fn instantiate_dyn(&self, commands: &EntityCommands) -> Vec<Entity> {
        let mut renderables = (
            // Models
            Vec::default(),
            // Renderables
            Vec::default(),
        );

        let mut lights = (
            // Models
            Vec::default(),
            // Lights
            Vec::default(),
        );

        fn traverse(
            parent_model: Mat4,
            node: &Node,
            asset: &ModelAsset,
            renderables: &mut (Vec<Model>, Vec<Renderable>),
            lights: &mut (Vec<Model>, Vec<PointLight>),
        ) {
            match &node.data {
                NodeData::Empty => {}
                NodeData::MeshGroup(idx) => {
                    let mesh_group = &asset.mesh_groups[*idx];
                    for instance in &mesh_group.0 {
                        let material = &asset.materials[instance.material];
                        renderables.0.push(Model(parent_model * node.model));
                        renderables.1.push(Renderable {
                            mesh: instance.mesh.clone(),
                            material: material.clone(),
                            layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                        });
                    }
                }
                NodeData::Light(index) => {
                    let light = &asset.lights[*index];
                    match light {
                        Light::Point(light) => {
                            lights.0.push(Model(parent_model * node.model));
                            lights.1.push(*light);
                        }
                        _ => {}
                    }
                }
            }

            for root in &node.children {
                traverse(node.model, root, asset, renderables, lights);
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut renderables, &mut lights);
        }

        let mut entities = Vec::with_capacity(renderables.0.len() + lights.0.len());
        let light_offset = renderables.0.len();

        entities.resize(renderables.0.len() + lights.0.len(), Entity::null());
        commands.create(renderables, &mut entities);
        commands.create(lights, &mut entities[light_offset..]);

        entities
    }

    pub fn instantiate_static(
        &self,
        static_geo: &StaticGeometry,
        commands: &EntityCommands,
    ) -> (Vec<StaticRenderableHandle>, Vec<Entity>) {
        let mut renderables = Vec::default();

        let mut lights = (
            // Models
            Vec::default(),
            // Lights
            Vec::default(),
        );

        fn traverse(
            parent_model: Mat4,
            node: &Node,
            asset: &ModelAsset,
            renderables: &mut Vec<StaticRenderable>,
            lights: &mut (Vec<Model>, Vec<PointLight>),
        ) {
            match &node.data {
                NodeData::Empty => {}
                NodeData::MeshGroup(mesh_group) => {
                    let mesh_group = &asset.mesh_groups[*mesh_group];
                    for instance in &mesh_group.0 {
                        let material = &asset.materials[instance.material];
                        renderables.push(StaticRenderable {
                            renderable: Renderable {
                                mesh: instance.mesh.clone(),
                                material: material.clone(),
                                layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                            },
                            model: Model(parent_model * node.model),
                            entity: Entity::null(),
                        });
                    }
                }
                NodeData::Light(index) => {
                    let light = &asset.lights[*index];
                    match light {
                        Light::Point(light) => {
                            lights.0.push(Model(parent_model * node.model));
                            lights.1.push(*light);
                        }
                        _ => {}
                    }
                }
            }

            for child in &node.children {
                traverse(node.model, child, asset, renderables, lights);
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut renderables, &mut lights);
        }

        let handles = static_geo.register(&renderables);
        let mut entities = Vec::with_capacity(lights.0.len());
        entities.resize(lights.0.len(), Entity::null());
        commands.create(lights, &mut entities);

        (handles, entities)
    }
}
