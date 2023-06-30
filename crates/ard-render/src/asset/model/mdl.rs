use std::path::{Path, PathBuf};

use ard_assets::prelude::*;
use ard_formats::{
    material::{BlendType, MaterialType},
    model::ModelHeader,
};
use ard_log::warn;
use tokio_stream::StreamExt;

use crate::{
    asset::model::{Light, MeshGroup, MeshInstance, ModelPostLoad, Node, NodeData},
    lighting::PointLight,
    material::MaterialInstanceCreateInfo,
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    pbr::{
        PbrMaterial, PBR_DIFFUSE_MAP_SLOT, PBR_METALLIC_ROUGHNESS_MAP_SLOT, PBR_NORMAL_MAP_SLOT,
    },
    texture::{MipType, Sampler, TextureCreateInfo},
};

use super::{ModelAsset, ModelLoader};

pub async fn to_asset(
    path: &Path,
    package: &Package,
    loader: &ModelLoader,
) -> Result<ModelAsset, AssetLoadError> {
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
        node_count: 0,
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

    model.lights = Vec::with_capacity(header.lights.len());
    let mut lights_iter = tokio_stream::iter(header.lights.iter());
    while let Some(light) = lights_iter.next().await {
        model.lights.push(match light {
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
        });
    }

    // Read texutres in parallel
    let mut textures_path = PathBuf::from(path);
    textures_path.push("textures");

    let texture_paths = (0..header.textures.len())
        .map(|i| {
            let mut path = textures_path.clone();
            path.push(format!("{i}"));
            path.push(format!("{}", header.textures[i].mip_count - 1));
            path
        })
        .collect::<Vec<_>>();

    let texture_results = futures::future::join_all(
        (0..header.textures.len()).map(|i| package.read(&texture_paths[i])),
    )
    .await;

    let mut texture_data = Vec::with_capacity(texture_results.len());
    for result in texture_results {
        texture_data.push(result?);
    }

    // Create textures using lowest level mip
    model.textures = Vec::with_capacity(header.textures.len());
    let mut textures_iter = tokio_stream::iter(header.textures.iter());
    let mut i = 0;

    while let Some(texture) = textures_iter.next().await {
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

        model
            .textures
            .push(loader.factory.create_texture(create_info));
        i += 1;
    }

    // Load in mesh group data in parallel
    let mut mg_path = PathBuf::from(path);
    mg_path.push("mesh_groups");

    let mg_paths = (0..header.mesh_groups.len())
        .map(|i| {
            (0..header.mesh_groups[i].0.len())
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

    let mg_vetices_results = futures::future::join_all((0..mg_paths.len()).map(|i| {
        futures::future::join_all((0..mg_paths[i].len()).map(|j| package.read(&mg_paths[i][j].0)))
    }))
    .await;

    let mg_indices_results = futures::future::join_all((0..mg_paths.len()).map(|i| {
        futures::future::join_all((0..mg_paths[i].len()).map(|j| package.read(&mg_paths[i][j].1)))
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
    model.mesh_groups = Vec::with_capacity(header.mesh_groups.len());
    let mut mesh_groups_iter = tokio_stream::iter(header.mesh_groups.iter());
    let mut mesh_group_id = 0;

    while let Some(mesh_group) = mesh_groups_iter.next().await {
        let mut instances = Vec::with_capacity(mg_data[mesh_group_id].len());

        let mut instance_iter = tokio_stream::iter(mg_data[mesh_group_id].iter());
        let mut instance_id = 0;
        while let Some(instance) = instance_iter.next().await {
            let create_info = MeshCreateInfo {
                bounds: MeshBounds::Generate,
                indices: bytemuck::cast_slice(&instance.1),
                vertices: Vertices::Combined {
                    data: &instance.0,
                    count: mesh_group.0[instance_id].mesh.vertex_count as usize,
                    layout: mesh_group.0[instance_id].mesh.vertex_layout,
                },
            };
            let mesh = loader.factory.create_mesh(create_info);
            instances.push(MeshInstance {
                mesh,
                material: mesh_group.0[instance_id].material as usize,
            });
            instance_id += 1;
        }
        model.mesh_groups.push(MeshGroup(instances));
        mesh_group_id += 1;
    }

    model.materials = Vec::with_capacity(header.materials.len());
    let mut materials_iter = tokio_stream::iter(header.materials.iter());

    while let Some(material) = materials_iter.next().await {
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
                let material_instance = loader.factory.create_material_instance(create_info);

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

                model.materials.push(material_instance);
            }
        }
    }

    fn parse_node(node: ard_formats::model::Node) -> (Node, usize) {
        let mut node_count = 1;

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

    Ok(model)
}
