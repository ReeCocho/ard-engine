use ard_gltf::{BlendType, GltfModel, GltfNode, GltfNodeData};
use ard_log::warn;

use crate::{
    asset::model::{ModelPostLoad, Node, NodeData},
    lighting::PointLight,
    material::MaterialInstanceCreateInfo,
    mesh::{MeshBounds, MeshCreateInfo, Vertices},
    pbr::{
        PbrMaterial, PBR_DIFFUSE_MAP_SLOT, PBR_METALLIC_ROUGHNESS_MAP_SLOT, PBR_NORMAL_MAP_SLOT,
    },
    texture::{MipType, Sampler, TextureCreateInfo},
};

use super::{Light, MeshGroup, MeshInstance, ModelAsset, ModelLoader};

pub fn to_asset(gltf: GltfModel, loader: &ModelLoader) -> ModelAsset {
    use rayon::prelude::*;

    let mut model = ModelAsset {
        lights: Vec::default(),
        textures: Vec::default(),
        materials: Vec::default(),
        mesh_groups: Vec::default(),
        node_count: 0,
        roots: Vec::with_capacity(gltf.roots.len()),
        post_load: Some(ModelPostLoad::Gltf),
    };

    model.lights = gltf
        .lights
        .par_iter()
        .map(|light| match light {
            ard_gltf::GltfLight::Point {
                color,
                intensity,
                range,
            } => Light::Point(PointLight {
                color: *color,
                intensity: *intensity,
                range: *range,
            }),
            ard_gltf::GltfLight::Spot { .. } => {
                warn!("attempt to load spot light but not supported");
                Light::Spot
            }
            ard_gltf::GltfLight::Directional { .. } => {
                warn!("attempt to load directional light but not supported");
                Light::Directional
            }
        })
        .collect();

    model.textures = gltf
        .textures
        .par_iter()
        .map(|texture| {
            let image = image::load_from_memory_with_format(
                &texture.data,
                match texture.src_format {
                    ard_gltf::TextureSourceFormat::Jpeg => image::ImageFormat::Jpeg,
                    ard_gltf::TextureSourceFormat::Png => image::ImageFormat::Png,
                },
            )
            .unwrap();
            let raw = image.to_rgba8();
            let format = texture.usage.into_format();

            let create_info = TextureCreateInfo {
                width: image.width(),
                height: image.height(),
                format,
                data: &raw,
                mip_type: if texture.mips {
                    MipType::Generate
                } else {
                    MipType::Upload
                },
                mip_count: if texture.mips {
                    (image.width().max(image.height()) as f32).log2() as usize + 1
                } else {
                    1
                },
                sampler: Sampler {
                    min_filter: texture.sampler.min_filter,
                    mag_filter: texture.sampler.mag_filter,
                    mipmap_filter: texture.sampler.mipmap_filter,
                    address_u: texture.sampler.address_u,
                    address_v: texture.sampler.address_v,
                    anisotropy: true,
                },
            };

            loader.factory.create_texture(create_info)
        })
        .collect();

    let (materials, mesh_groups) = rayon::join(
        || {
            gltf.materials
                .par_iter()
                .map(|material| match material {
                    ard_gltf::GltfMaterial::Pbr {
                        base_color,
                        metallic,
                        roughness,
                        alpha_cutoff,
                        diffuse_map,
                        normal_map,
                        metallic_roughness_map,
                        blending,
                    } => {
                        if *blending == BlendType::Blend {
                            warn!("material requires blending but not supported");
                        }

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
                                Some(&model.textures[*diffuse_map]),
                                PBR_DIFFUSE_MAP_SLOT,
                            );
                        }

                        if let Some(normal_map) = normal_map {
                            loader.factory.update_material_texture(
                                &material_instance,
                                Some(&model.textures[*normal_map]),
                                PBR_NORMAL_MAP_SLOT,
                            );
                        }

                        if let Some(mr_map) = metallic_roughness_map {
                            loader.factory.update_material_texture(
                                &material_instance,
                                Some(&model.textures[*mr_map]),
                                PBR_METALLIC_ROUGHNESS_MAP_SLOT,
                            );
                        }

                        material_instance
                    }
                })
                .collect()
        },
        || {
            gltf.mesh_groups
                .par_iter()
                .map(|mesh_group| {
                    let out_mesh_group = MeshGroup(
                        mesh_group
                            .0
                            .par_iter()
                            .map(|instance| {
                                let create_info = MeshCreateInfo {
                                    bounds: MeshBounds::Generate,
                                    indices: &instance.mesh.indices,
                                    vertices: Vertices::Attributes {
                                        positions: &instance.mesh.positions,
                                        normals: instance.mesh.normals.as_deref(),
                                        tangents: instance.mesh.tangents.as_deref(),
                                        colors: instance.mesh.colors.as_deref(),
                                        uv0: instance.mesh.uv0.as_deref(),
                                        uv1: instance.mesh.uv1.as_deref(),
                                        uv2: instance.mesh.uv2.as_deref(),
                                        uv3: instance.mesh.uv3.as_deref(),
                                    },
                                };
                                let mesh = loader.factory.create_mesh(create_info);

                                MeshInstance {
                                    mesh,
                                    material: instance.material,
                                }
                            })
                            .collect(),
                    );

                    out_mesh_group
                })
                .collect()
        },
    );

    model.materials = materials;
    model.mesh_groups = mesh_groups;

    fn load_node(node: &GltfNode) -> (Node, usize) {
        let mut node_count = 1;

        let mut out_node = Node {
            name: node.name.clone(),
            model: node.model,
            children: Vec::with_capacity(node.children.len()),
            data: match &node.data {
                GltfNodeData::Empty => NodeData::Empty,
                GltfNodeData::Light(id) => NodeData::Light(*id),
                GltfNodeData::MeshGroup(id) => NodeData::MeshGroup(*id),
            },
        };

        for child in &node.children {
            let (node, child_count) = load_node(child);
            node_count += child_count;
            out_node.children.push(node);
        }

        (out_node, node_count)
    }

    for root in &gltf.roots {
        let (node, node_count) = load_node(root);
        model.node_count += node_count;
        model.roots.push(node);
    }

    model
}
