use std::path::{Path, PathBuf};

use ard_assets::prelude::*;
use ard_formats::{
    material::{BlendType, MaterialType},
    model::ModelHeader,
};
use ard_log::warn;

use crate::{
    asset::model::{Light, MeshGroup, MeshInstance, ModelPostLoad, Node, NodeData},
    lighting::PointLight,
    material::MaterialInstanceCreateInfo,
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    pbr::{
        PbrMaterial, PBR_DIFFUSE_MAP_SLOT, PBR_METALLIC_ROUGHNESS_MAP_SLOT, PBR_NORMAL_MAP_SLOT,
    },
    texture::{MipType, Sampler, Texture, TextureCreateInfo},
};

use super::{ModelAsset, ModelLoader};

pub async fn to_asset(
    path: &Path,
    package: &Package,
    loader: &ModelLoader,
) -> Result<ModelAsset, AssetLoadError> {
    use rayon::prelude::*;

    // NOTE: Mixing Rayon and Tokio has caused me to lose brain cells. A lot of this code is jank
    // purely so I could get it working. Fix this, because I hate it.

    // Load in the header
    let header = {
        let mut path = PathBuf::from(path);
        path.push("header");
        let data = package.read(&path).await?;
        match bincode::deserialize::<ModelHeader>(&data) {
            Ok(header) => header,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        }
    };

    let mut model = ModelAsset {
        lights: Vec::default(),
        textures: Vec::with_capacity(header.textures.len()),
        materials: Vec::default(),
        mesh_groups: Vec::with_capacity(header.mesh_groups.len()),
        roots: Vec::with_capacity(header.roots.len()),
        post_load: Some(ModelPostLoad::Ard {
            texture_path: {
                let mut path = PathBuf::from(path);
                path.push("textures");
                path
            },
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

    model.lights = header
        .lights
        .par_iter()
        .map(|light| match light {
            ard_formats::model::Light::Point {
                color,
                intensity,
                range,
            } => Light::Point(PointLight {
                color: *color,
                intensity: *intensity,
                range: *range,
            }),
            ard_formats::model::Light::Spot { .. } => {
                warn!("attempt to load spot light but unsupported");
                Light::Spot
            }
            ard_formats::model::Light::Directional { .. } => {
                warn!("attempt to load directional light but unsupported");
                Light::Directional
            }
        })
        .collect();

    // Read texutres in parallel
    let mut textures_path = PathBuf::from(path);
    textures_path.push("textures");

    let texture_paths = (0..header.textures.len())
        .into_iter()
        .map(|i| {
            let mut path = textures_path.clone();
            path.push(format!("{i}"));
            path.push(format!("{}", header.textures[i].mip_count - 1));
            path
        })
        .collect::<Vec<_>>();

    let texture_results = futures::future::join_all(
        (0..header.textures.len())
            .into_iter()
            .map(|i| package.read(&texture_paths[i])),
    )
    .await;

    let mut texture_data = Vec::with_capacity(texture_results.len());
    for result in texture_results {
        texture_data.push(result?);
    }

    // Create textures using lowest level mip
    let texture_results: Vec<Result<Texture, AssetLoadError>> = header
        .textures
        .par_iter()
        .enumerate()
        .map(|(i, texture)| {
            let create_info = TextureCreateInfo {
                width: texture.width,
                height: texture.height,
                format: texture.format,
                data: &texture_data[i],
                mip_type: MipType::Upload,
                mip_count: texture.mip_count as usize,
                sampler: Sampler {
                    min_filter: texture.sampler.min_filter,
                    mag_filter: texture.sampler.mag_filter,
                    mipmap_filter: texture.sampler.mipmap_filter,
                    address_u: texture.sampler.address_u,
                    address_v: texture.sampler.address_v,
                    anisotropy: true,
                },
            };

            Ok(loader.factory.create_texture(create_info))
        })
        .collect();

    for result in texture_results {
        model.textures.push(result?);
    }

    // Load in mesh group data in parallel
    let mut mg_path = PathBuf::from(path);
    mg_path.push("mesh_groups");

    let mg_paths = (0..header.mesh_groups.len())
        .into_iter()
        .map(|i| {
            (0..header.mesh_groups[i].0.len())
                .into_iter()
                .map(|j| {
                    let mut path = mg_path.clone();
                    path.push(format!("{i}"));
                    path.push(format!("{j}"));

                    let mut vertex_path = path.clone();
                    vertex_path.push("vertices");

                    let mut index_path = path.clone();
                    index_path.push("indices");

                    (vertex_path, index_path)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mg_vetices_results = futures::future::join_all((0..mg_paths.len()).into_iter().map(|i| {
        futures::future::join_all(
            (0..mg_paths[i].len())
                .into_iter()
                .map(|j| package.read(&mg_paths[i][j].0)),
        )
    }))
    .await;

    let mg_indices_results = futures::future::join_all((0..mg_paths.len()).into_iter().map(|i| {
        futures::future::join_all(
            (0..mg_paths[i].len())
                .into_iter()
                .map(|j| package.read(&mg_paths[i][j].1)),
        )
    }))
    .await;

    let mut mg_data = Vec::with_capacity(mg_vetices_results.len());
    for (verts, inds) in mg_vetices_results.into_iter().zip(mg_indices_results) {
        let mut data = Vec::with_capacity(verts.len());
        for (vert_result, ind_result) in verts.into_iter().zip(inds) {
            data.push((vert_result?, ind_result?));
        }
        mg_data.push(data);
    }

    // Create materials and mesh groups in parallel
    let (mesh_group_results, materials) = rayon::join(
        || {
            header
                .mesh_groups
                .par_iter()
                .enumerate()
                .map(|(mesh_group_id, mesh_group)| {
                    let instance_results = mesh_group
                        .0
                        .par_iter()
                        .enumerate()
                        .map(|(instance_id, instance)| {
                            let create_info = MeshCreateInfo {
                                bounds: MeshBounds::Generate,
                                indices: bytemuck::cast_slice(
                                    &mg_data[mesh_group_id][instance_id].1,
                                ),
                                vertices: Vertices::Combined {
                                    data: &mg_data[mesh_group_id][instance_id].0,
                                    count: instance.mesh.vertex_count as usize,
                                    layout: instance.mesh.vertex_layout,
                                },
                            };
                            let mesh = loader.factory.create_mesh(create_info);

                            Ok(MeshInstance {
                                mesh,
                                material: instance.material as usize,
                            })
                        })
                        .collect::<Vec<Result<MeshInstance, AssetLoadError>>>();

                    let mut instances = Vec::with_capacity(instance_results.len());
                    for result in instance_results {
                        instances.push(result?);
                    }

                    Ok(MeshGroup(instances))
                })
                .collect::<Vec<Result<MeshGroup, AssetLoadError>>>()
        },
        || {
            header
                .materials
                .par_iter()
                .map(|material| {
                    if material.blend_ty == BlendType::Blend {
                        warn!("material requires blending but not supported");
                    }

                    match &material.ty {
                        MaterialType::Pbr {
                            base_color,
                            metallic,
                            roughness,
                            alpha_cutoff,
                            diffuse_map,
                            normal_map,
                            metallic_roughness_map,
                        } => {
                            let create_info = MaterialInstanceCreateInfo {
                                material: loader.pbr_material.clone(),
                            };
                            let material_instance =
                                loader.factory.create_material_instance(create_info);

                            loader.factory.update_material_data(
                                &material_instance,
                                &PbrMaterial {
                                    base_color: *base_color,
                                    metallic: *metallic,
                                    roughness: *roughness,
                                    alpha_cutoff: *alpha_cutoff,
                                },
                            );

                            if let Some(diffuse_map) = diffuse_map {
                                loader.factory.update_material_texture(
                                    &material_instance,
                                    Some(&model.textures[*diffuse_map as usize]),
                                    PBR_DIFFUSE_MAP_SLOT,
                                );
                            }

                            if let Some(normal_map) = normal_map {
                                loader.factory.update_material_texture(
                                    &material_instance,
                                    Some(&model.textures[*normal_map as usize]),
                                    PBR_NORMAL_MAP_SLOT,
                                );
                            }

                            if let Some(mr_map) = metallic_roughness_map {
                                loader.factory.update_material_texture(
                                    &material_instance,
                                    Some(&model.textures[*mr_map as usize]),
                                    PBR_METALLIC_ROUGHNESS_MAP_SLOT,
                                );
                            }

                            material_instance
                        }
                    }
                })
                .collect()
        },
    );

    for result in mesh_group_results {
        model.mesh_groups.push(result?);
    }

    model.materials = materials;

    fn parse_node(node: ard_formats::model::Node) -> Node {
        let mut out_node = Node {
            name: node.name,
            model: node.model,
            children: Vec::with_capacity(node.children.len()),
            data: match node.data {
                ard_formats::model::NodeData::Empty => NodeData::Empty,
                ard_formats::model::NodeData::MeshGroup(id) => NodeData::MeshGroup(id as usize),
                ard_formats::model::NodeData::Light(id) => NodeData::Light(id as usize),
            },
        };

        for child in node.children {
            out_node.children.push(parse_node(child));
        }

        out_node
    }

    for root in header.roots {
        model.roots.push(parse_node(root));
    }

    Ok(model)
}
