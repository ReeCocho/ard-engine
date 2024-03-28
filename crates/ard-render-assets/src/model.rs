use std::path::PathBuf;

use ard_assets::prelude::*;
use ard_formats::{
    material::{BlendType, MaterialType},
    mesh::{MeshData, MeshHeader},
    model::{MeshGroup, ModelHeader},
    texture::{MipType, Sampler, TextureData, TextureHeader},
};
use ard_math::Mat4;
use ard_render::factory::Factory;
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::{Mesh, MeshCreateInfo};
use ard_render_objects::{Model, RenderFlags};
use ard_render_pbr::{
    PbrMaterialData, PBR_MATERIAL_DIFFUSE_SLOT, PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT,
    PBR_MATERIAL_NORMAL_SLOT,
};
use ard_render_textures::texture::{Texture, TextureCreateInfo};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct ModelLoader {
    factory: Factory,
}

pub struct ModelAsset {
    // pub lights: Vec<Light>,
    pub textures: Vec<Texture>,
    pub materials: Vec<ModelMaterialInstance>,
    pub mesh_groups: Vec<MeshGroup>,
    pub meshes: Vec<Mesh>,
    pub node_count: usize,
    pub roots: Vec<Node>,
    post_load: Option<ModelPostLoad>,
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

#[derive(Serialize, Deserialize)]
pub struct ModelAssetHeader {
    pub path: PathBuf,
}

struct ModelPostLoad {
    /// Path to the asset.
    asset_root: PathBuf,
    /// First element is the index of the texture. The second element is the mip level that
    /// needs to be loaded next. Elements are sorted largest to smallest by the mip level so
    /// they can be popped when all mips are loaded.
    texture_mips: Vec<(usize, u32)>,
}

// pub enum Light {
//     Point(PointLight),
//     Spot,
//     Directional,
// }

pub struct ModelMaterialInstance {
    pub instance: MaterialInstance,
    pub render_mode: RenderingMode,
}

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
    const EXTENSION: &'static str = "model";
    type Loader = ModelLoader;
}

impl ModelLoader {
    pub fn new(factory: Factory) -> Self {
        Self { factory }
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
        // Read in the asset header
        let header = package.read_str(asset.to_owned()).await?;
        let header = match ron::from_str::<ModelAssetHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Read in the model header
        let model_path = header.path;
        let header = package.read(ModelHeader::header_path(&model_path)).await?;
        let header = match bincode::deserialize::<ModelHeader>(&header) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Begin constructing the asset
        let mut model = ModelAsset {
            // lights: Vec::default(),
            textures: Vec::default(),
            materials: Vec::default(),
            mesh_groups: header.mesh_groups.clone(),
            meshes: Vec::default(),
            roots: Vec::with_capacity(header.roots.len()),
            node_count: 0,
            post_load: Some(ModelPostLoad {
                asset_root: model_path.clone(),
                texture_mips: {
                    let mut mips = Vec::with_capacity(header.textures.len());
                    for (i, texture) in header.textures.iter().enumerate() {
                        if texture.mip_count > 1 {
                            // First mip is already loaded, so we must load second
                            mips.push((i, texture.mip_count - 2));
                        }
                    }
                    mips.sort_unstable_by_key(|(_, mip_count)| *mip_count);
                    mips.reverse();
                    mips
                },
            }),
        };

        // Load in resources for the model
        let (textures, meshes) = futures::future::try_join(
            self.load_textures(&header, &package, &model_path),
            self.load_meshes(&header, &package, &model_path),
        )
        .await?;

        let materials = self.load_materials(&header, &textures).await?;

        model.textures = textures;
        model.meshes = meshes;
        model.materials = materials;

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

        Ok(AssetLoadResult::NeedsPostLoad {
            asset: model,
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
        let mut post_load_info = assets.get_mut(&handle).unwrap().post_load.take().unwrap();

        let mips = std::mem::take(&mut post_load_info.texture_mips);
        futures::future::join_all(mips.into_iter().map(|(texture_idx, mip_count)| {
            let asset_root = post_load_info.asset_root.clone();
            let package = package.clone();
            let assets = assets.clone();
            let handle = handle.clone();
            async move {
                for mip in (0..=mip_count).rev() {
                    // Load mip
                    let tex_path = ModelHeader::texture_path(&asset_root, texture_idx);
                    let mip_path = TextureHeader::mip_path(tex_path, mip);
                    let data = match package.read(mip_path).await {
                        Ok(data) => data,
                        Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                    };

                    // Parse bincode
                    let data = bincode::deserialize::<TextureData>(&data).unwrap();

                    // Update mip
                    self.factory.load_texture_mip(
                        &assets.get(&handle).unwrap().textures[texture_idx],
                        mip as usize,
                        data,
                    );
                }

                Ok(())
            }
        }))
        .await
        .into_iter()
        .collect::<Result<(), AssetLoadError>>()?;

        Ok(AssetPostLoadResult::Loaded)
    }
}

impl ModelLoader {
    async fn load_textures(
        &self,
        header: &ModelHeader,
        package: &Package,
        asset_path: &AssetName,
    ) -> Result<Vec<Texture>, AssetLoadError> {
        // Read in the lowest detail level mip of every texture
        futures::future::join_all((0..header.textures.len()).map(|i| {
            async move {
                let tex_desc = &header.textures[i];

                // Read in the texture data
                let tex_path = ModelHeader::texture_path(asset_path, i);
                let path = TextureHeader::mip_path(tex_path, header.textures[i].mip_count - 1);
                let data = package.read(path).await?;

                // Decode texture data from bincode
                let source = match bincode::deserialize::<TextureData>(&data) {
                    Ok(data) => data,
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                };

                // Create texture
                let create_info = TextureCreateInfo {
                    source,
                    debug_name: Some(format!("{asset_path:?}/texture_{i}")),
                    mip_count: tex_desc.mip_count as usize,
                    mip_type: MipType::Upload(tex_desc.width, tex_desc.height),
                    sampler: Sampler {
                        min_filter: tex_desc.sampler.min_filter,
                        mag_filter: tex_desc.sampler.mag_filter,
                        mipmap_filter: tex_desc.sampler.mipmap_filter,
                        address_u: tex_desc.sampler.address_u,
                        address_v: tex_desc.sampler.address_v,
                        anisotropy: true,
                    },
                };

                match self.factory.create_texture(create_info) {
                    Ok(texture) => Ok(texture),
                    Err(err) => Err(AssetLoadError::Other(err.to_string())),
                }
            }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    }

    async fn load_meshes(
        &self,
        header: &ModelHeader,
        package: &Package,
        asset_path: &AssetName,
    ) -> Result<Vec<Mesh>, AssetLoadError> {
        // Read in mesh instances for each mesh group
        futures::future::join_all((0..header.meshes.len()).map(|mesh_idx| {
            async move {
                let mesh_path = ModelHeader::mesh_path(asset_path, mesh_idx);
                let mesh_data_path = MeshHeader::mesh_data_path(mesh_path);

                let mdata = package.read(mesh_data_path).await?;

                // Decode mesh data from bincode
                let mdata = match bincode::deserialize::<MeshData>(&mdata) {
                    Ok(data) => data,
                    Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                };

                // Create mesh
                let create_info = MeshCreateInfo {
                    debug_name: Some(format!("{asset_path:?}/meshes/{mesh_idx}")),
                    data: mdata,
                };

                match self.factory.create_mesh(create_info) {
                    Ok(mesh) => Ok(mesh),
                    Err(err) => Err(AssetLoadError::Other(err.to_string())),
                }
            }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    }

    async fn load_materials(
        &self,
        header: &ModelHeader,
        textures: &[Texture],
    ) -> Result<Vec<ModelMaterialInstance>, AssetLoadError> {
        Ok(
            futures::future::join_all(header.materials.iter().map(|mat_header| async {
                match &mat_header.ty {
                    MaterialType::Pbr {
                        base_color,
                        metallic,
                        roughness,
                        alpha_cutoff,
                        diffuse_map,
                        normal_map,
                        metallic_roughness_map,
                    } => {
                        let instance = match self.factory.create_pbr_material_instance() {
                            Ok(instance) => instance,
                            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
                        };

                        // Apply material properties
                        self.factory.set_material_data(
                            &instance,
                            &PbrMaterialData {
                                alpha_cutoff: *alpha_cutoff,
                                color: *base_color,
                                metallic: *metallic,
                                roughness: *roughness,
                            },
                        );

                        // Apply material textures
                        if let Some(tex) = diffuse_map {
                            self.factory.set_material_texture_slot(
                                &instance,
                                PBR_MATERIAL_DIFFUSE_SLOT,
                                Some(&textures[*tex as usize]),
                            );
                        }

                        if let Some(tex) = normal_map {
                            self.factory.set_material_texture_slot(
                                &instance,
                                PBR_MATERIAL_NORMAL_SLOT,
                                Some(&textures[*tex as usize]),
                            );
                        }

                        if let Some(tex) = metallic_roughness_map {
                            self.factory.set_material_texture_slot(
                                &instance,
                                PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT,
                                Some(&textures[*tex as usize]),
                            );
                        }

                        Ok(ModelMaterialInstance {
                            instance,
                            render_mode: match mat_header.blend_ty {
                                BlendType::Opaque => RenderingMode::Opaque,
                                BlendType::Mask => RenderingMode::AlphaCutout,
                                BlendType::Blend => RenderingMode::Transparent,
                            },
                        })
                    }
                }
            }))
            .await
            // Check for errors
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect(),
        )
    }
}

impl ModelAsset {
    pub fn instantiate(&self) -> ModelAssetInstance {
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
        ) {
            match &node.data {
                NodeData::Empty => {}
                NodeData::MeshGroup(mesh_group) => {
                    let mesh_group = &asset.mesh_groups[*mesh_group];
                    for instance in &mesh_group.0 {
                        let material = &asset.materials[instance.material as usize];
                        let mesh = &asset.meshes[instance.mesh as usize];

                        meshes.meshes.push(mesh.clone());
                        meshes.materials.push(material.instance.clone());
                        meshes.models.push(Model(parent_model * node.model.0));
                        meshes.flags.push(RenderFlags::SHADOW_CASTER);
                        meshes.rendering_mode.push(material.render_mode);
                    }
                }
            }

            for child in &node.children {
                traverse(node.model.0, child, asset, meshes);
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut meshes);
        }

        ModelAssetInstance { meshes }
    }
}
