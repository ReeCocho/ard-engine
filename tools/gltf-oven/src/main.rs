use std::fs;
use std::io::BufWriter;
use std::ops::Div;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use ard_assets::asset::{AssetName, AssetNameBuf};
use ard_formats::material::{BlendType, MaterialHeader, MaterialType};
use ard_formats::mesh::{MeshDataBuilder, MeshHeader};
use ard_formats::model::{Light, MeshGroup, MeshInstance, ModelHeader, Node, NodeData};
use ard_formats::texture::{Sampler, TextureData, TextureHeader};
use ard_formats::vertex::VertexLayout;
use ard_gltf::{GltfLight, GltfMesh, GltfTexture};
use ard_math::{Mat4, Vec2, Vec3, Vec4};
use ard_pal::prelude::Format;
use ard_transform::Model;
use clap::Parser;
use image::GenericImageView;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the model to bake.
    #[arg(short, long)]
    path: PathBuf,
    /// Output path for the model.
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Compute tangents based on UVs.
    #[arg(long, default_value_t = false)]
    compute_tangents: bool,
    /// Compress textures.
    #[arg(long, default_value_t = false)]
    compress_textures: bool,
    /// Use UUID file names.
    #[arg(long, default_value_t = false)]
    uuid_names: bool,
}

fn main() {
    let args = Args::parse();

    // Load in the model
    println!("Loading model...");
    let bin = fs::read(&args.path).unwrap();

    // Output folder path
    let out_path = match &args.out {
        Some(path) => path.clone(),
        None => {
            let mut out_path = PathBuf::from(args.path.parent().unwrap());
            out_path.push(format!(
                "{}.ard_mdl",
                args.path.file_stem().unwrap().to_str().unwrap()
            ));
            out_path
        }
    };
    let out_path = AssetNameBuf::from_path_buf(out_path).unwrap();

    std::fs::create_dir_all(&out_path).unwrap();

    // Parse the model
    println!("Parsing model...");
    let mut model = ard_gltf::GltfModel::from_slice(&bin).unwrap();
    std::mem::drop(bin);

    // For each texture, we mark if it was used in a way that needs a UNORM color format and not
    // SRGB.
    let texture_is_unorm: Vec<_> = model
        .textures
        .iter()
        .map(|_| AtomicBool::new(false))
        .collect();

    // We also precompute their header paths so materials know how to reference them
    let texture_paths: Vec<_> = model
        .textures
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let tex_path = ModelHeader::texture_path(&out_path, i);

            if args.uuid_names {
                let mut header_path = out_path.clone();
                header_path.push(format!("{}.ard_tex", uuid::Uuid::new_v4().to_string()));
                header_path
            } else {
                std::fs::create_dir_all(&tex_path).unwrap();
                TextureHeader::header_path(&tex_path)
            }
        })
        .collect();

    println!("Constructing header...");
    let mut header = create_header(&args, &out_path, &model, &texture_is_unorm, &texture_paths);

    // Save everything
    println!("Saving meshes and textures...");
    let (mesh_headers, _) = rayon::join(
        || save_meshes(&args, &out_path, std::mem::take(&mut model.meshes)),
        || {
            save_textures(
                &args,
                &out_path,
                std::mem::take(&mut model.textures),
                &texture_is_unorm,
                &texture_paths,
            )
        },
    );

    // Save the header
    header.meshes = mesh_headers;
    let header_path = if args.uuid_names {
        let mut path = out_path.clone();
        path.push(format!("{}.ard_mdl", uuid::Uuid::new_v4().to_string()));
        path
    } else {
        ModelHeader::header_path(out_path.clone())
    };
    let mut f = BufWriter::new(fs::File::create(&header_path).unwrap());
    bincode::serialize_into(&mut f, &header).unwrap();
    std::mem::drop(f);
    std::mem::drop(header);
}

fn create_header(
    args: &Args,
    out_path: &AssetName,
    gltf: &ard_gltf::GltfModel,
    texture_is_unorm: &[AtomicBool],
    texture_paths: &[AssetNameBuf],
) -> ModelHeader {
    let mut header = ModelHeader::default();
    header.lights = Vec::with_capacity(gltf.lights.len());
    header.materials = Vec::with_capacity(gltf.materials.len());
    header.mesh_groups = Vec::with_capacity(gltf.mesh_groups.len());
    header.textures = Vec::with_capacity(gltf.textures.len());
    header.roots = Vec::with_capacity(1);

    for light in &gltf.lights {
        header.lights.push(match light {
            GltfLight::Point {
                color,
                intensity,
                range,
            } => Light::Point {
                color: *color,
                intensity: *intensity,
                range: *range,
            },
            GltfLight::Spot {
                color,
                intensity,
                range,
                inner_angle,
                outer_angle,
            } => Light::Spot {
                color: *color,
                intensity: *intensity,
                range: *range,
                inner_angle: *inner_angle,
                outer_angle: *outer_angle,
            },
            GltfLight::Directional { color, intensity } => Light::Directional {
                color: *color,
                intensity: *intensity,
            },
        });
    }

    if !args.uuid_names {
        let mut mat_root = ModelHeader::material_path(out_path, 0);
        mat_root.pop();
        std::fs::create_dir_all(&mat_root).unwrap();
    }

    for (i, material) in gltf.materials.iter().enumerate() {
        let mat_header = match material {
            ard_gltf::GltfMaterial::Pbr {
                base_color,
                metallic,
                roughness,
                alpha_cutoff,
                diffuse_map,
                normal_map,
                metallic_roughness_map,
                blending,
            } => MaterialHeader {
                blend_ty: match *blending {
                    ard_gltf::BlendType::Opaque => BlendType::Opaque,
                    ard_gltf::BlendType::Mask => BlendType::Mask,
                    ard_gltf::BlendType::Blend => BlendType::Blend,
                },
                ty: MaterialType::Pbr {
                    base_color: *base_color,
                    metallic: *metallic,
                    roughness: *roughness,
                    alpha_cutoff: *alpha_cutoff,
                    diffuse_map: diffuse_map
                        .map(|v| PathBuf::from(texture_paths[v].file_name().unwrap())),
                    normal_map: normal_map.map(|v| {
                        texture_is_unorm[v].store(true, Ordering::Relaxed);
                        PathBuf::from(texture_paths[v].file_name().unwrap())
                    }),
                    metallic_roughness_map: metallic_roughness_map.map(|v| {
                        texture_is_unorm[v].store(true, Ordering::Relaxed);
                        PathBuf::from(texture_paths[v].file_name().unwrap())
                    }),
                },
            },
        };

        let mat_path = if args.uuid_names {
            let mut mat_path = AssetNameBuf::from(out_path);
            mat_path.push(format!("{}.ard_mat", uuid::Uuid::new_v4().to_string()));
            mat_path
        } else {
            ModelHeader::material_path(out_path, i)
        };
        let mut f = BufWriter::new(fs::File::create(&mat_path).unwrap());
        bincode::serialize_into(&mut f, &mat_header).unwrap();

        header.materials.push(if args.uuid_names {
            AssetNameBuf::from(mat_path.file_name().unwrap())
        } else {
            mat_path
        });
    }

    for mesh_group in &gltf.mesh_groups {
        let mut instances = Vec::with_capacity(mesh_group.0.len());
        for instance in &mesh_group.0 {
            instances.push(MeshInstance {
                material: instance.material as u32,
                mesh: instance.mesh as u32,
            });
        }

        header.mesh_groups.push(MeshGroup(instances));
    }

    fn parse_node(node: &ard_gltf::GltfNode) -> Node {
        let model = Model(node.model);
        let mut new_rot = model.rotation();
        let mut new_pos = model.position();
        let scale = model.scale();

        new_rot.y = -new_rot.y;
        new_rot.z = -new_rot.z;
        new_pos.x = -new_pos.x;

        let model = Mat4::from_scale_rotation_translation(scale.into(), new_rot, new_pos.into());

        let mut out_node = Node {
            name: node.name.clone(),
            model,
            data: match &node.data {
                ard_gltf::GltfNodeData::Empty => NodeData::Empty,
                ard_gltf::GltfNodeData::MeshGroup(id) => NodeData::MeshGroup(*id as u32),
                ard_gltf::GltfNodeData::Light(id) => NodeData::Light(*id as u32),
            },
            children: Vec::with_capacity(node.children.len()),
        };

        for child in &node.children {
            out_node.children.push(parse_node(child));
        }

        out_node
    }

    // If we have one root, we can make that the output root
    if gltf.roots.len() == 1 {
        header.roots.push(parse_node(gltf.roots.last().unwrap()));
    }
    // Otherwise, we create an extra empty node to act as a root and put all of the GLTF roots
    // into it
    else if gltf.roots.len() > 1 {
        let mut root = Node {
            name: "__root".into(),
            model: Mat4::IDENTITY,
            data: NodeData::Empty,
            children: Vec::with_capacity(gltf.roots.len()),
        };

        for child in &gltf.roots {
            root.children.push(parse_node(child));
        }

        header.roots.push(root);
    }

    header
}

fn save_meshes(args: &Args, out: &AssetName, meshes: Vec<GltfMesh>) -> Vec<AssetNameBuf> {
    use rayon::prelude::*;
    meshes
        .into_par_iter()
        .enumerate()
        .map(|(i, mesh)| {
            let mesh_path = if args.uuid_names {
                AssetNameBuf::from(out)
            } else {
                let mesh_path = ModelHeader::mesh_path(out, i);
                fs::create_dir_all(&mesh_path).unwrap();
                mesh_path
            };
            save_mesh(args, &mesh_path, mesh)
        })
        .collect()
}

fn save_mesh(args: &Args, out: &AssetName, mut mesh: GltfMesh) -> AssetNameBuf {
    let (mesh_data_path, mesh_header_path) = if args.uuid_names {
        let mut mesh_data_path = AssetNameBuf::from(out);
        mesh_data_path.push(uuid::Uuid::new_v4().to_string());

        let mut mesh_header_path = AssetNameBuf::from(out);
        mesh_header_path.push(format!("{}.ard_msh", uuid::Uuid::new_v4().to_string()));

        (mesh_data_path, mesh_header_path)
    } else {
        (
            MeshHeader::mesh_data_path(out),
            MeshHeader::mesh_header_path(out),
        )
    };

    let mut vertex_layout = VertexLayout::POSITION | VertexLayout::NORMAL;

    // GLTF is left handed, so we need to convert everything to right handed
    let inv_handed = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    mesh.positions.iter_mut().for_each(|p| *p = inv_handed * *p);
    mesh.normals = mesh.normals.map(|mut n| {
        n.iter_mut().for_each(|p| *p = inv_handed * *p);
        n
    });
    mesh.tangents = mesh.tangents.map(|mut t| {
        t.iter_mut().for_each(|p| *p = inv_handed * *p);
        t
    });

    if mesh.tangents.is_some() || (args.compute_tangents && mesh.uv0.is_some()) {
        vertex_layout |= VertexLayout::TANGENT;
    }

    if mesh.uv0.is_some() {
        vertex_layout |= VertexLayout::UV0;
    }

    if mesh.uv1.is_some() {
        vertex_layout |= VertexLayout::UV1;
    }

    // Build vertex data
    let mut mesh_data =
        MeshDataBuilder::new(vertex_layout, mesh.positions.len(), mesh.indices.len());

    mesh_data = mesh_data
        .add_positions(&mesh.positions)
        .add_indices(&mesh.indices);

    mesh_data = match &mesh.normals {
        Some(normals) => mesh_data.add_vec4_normals(normals),
        None => {
            println!("WARNING: Vertices at {mesh_data_path:?} are missing normals. Generating dummy normals...");
            mesh_data.add_vec4_normals(&vec![Vec4::new(0.0, 0.0, 1.0, 0.0); mesh.positions.len()])
        }
    };
    mesh.normals = None;

    // Check if we can compute tangents
    if args.compute_tangents {
        if let Some(uvs) = &mesh.uv0 {
            let tangents = compute_tangents(&mesh.positions, uvs, &mesh.indices);
            mesh_data = mesh_data.add_vec4_tangents(&tangents);
        }
    } else {
        if let Some(tangents) = &mesh.tangents {
            mesh_data = mesh_data.add_vec4_tangents(&tangents);
        }
    }
    mesh.tangents = None;

    // Clear these here since it's possible they might be used to compute tangents
    mesh.positions = Vec::default();
    mesh.indices = Vec::default();

    if let Some(uv0) = &mesh.uv0 {
        mesh_data = mesh_data.add_vec2_uvs(&uv0, 0);
    }
    mesh.uv0 = None;

    if let Some(uv1) = &mesh.uv1 {
        mesh_data = mesh_data.add_vec2_uvs(&uv1, 1);
    }
    mesh.uv1 = None;

    // Save the buffer
    let data = mesh_data.build();
    let mut f = BufWriter::new(fs::File::create(&mesh_data_path).unwrap());
    bincode::serialize_into(&mut f, &data).unwrap();

    // Save the header
    let header = MeshHeader {
        data_path: AssetNameBuf::from(mesh_data_path.file_name().unwrap()),
        index_count: data.index_count() as u32,
        vertex_count: data.vertex_count() as u32,
        meshlet_count: data.meshlet_count() as u32,
        vertex_layout,
    };
    let mut f = BufWriter::new(fs::File::create(&mesh_header_path).unwrap());
    bincode::serialize_into(&mut f, &header).unwrap();

    AssetNameBuf::from(mesh_header_path.file_name().unwrap())
}

fn save_textures(
    args: &Args,
    out: &AssetName,
    textures: Vec<GltfTexture>,
    texture_is_unorm: &[AtomicBool],
    texture_paths: &[AssetNameBuf],
) {
    use rayon::prelude::*;
    textures
        .into_par_iter()
        .enumerate()
        .for_each(|(i, mut texture)| {
            // Parse the image
            let image_fmt = match texture.src_format {
                ard_gltf::TextureSourceFormat::Png => image::ImageFormat::Png,
                ard_gltf::TextureSourceFormat::Jpeg => image::ImageFormat::Jpeg,
            };
            let image = image::load_from_memory_with_format(&texture.data, image_fmt).unwrap();
            texture.data = Vec::default();

            let compress = args.compress_textures && texture_needs_compression(&image);
            let mip_count = texture_mip_count(&image, compress);
            let format = if compress {
                if texture_is_unorm[i].load(Ordering::Relaxed) {
                    Format::BC7Unorm
                } else {
                    Format::BC7Srgb
                }
            } else {
                if texture_is_unorm[i].load(Ordering::Relaxed) {
                    Format::Rgba8Unorm
                } else {
                    Format::Rgba8Srgb
                }
            };

            let tex_path = ModelHeader::texture_path(out, i);
            let header_path = texture_paths[i].clone();

            let tex_header = TextureHeader {
                width: image.width(),
                height: image.height(),
                mips: (0..mip_count)
                    .map(|mip| {
                        if args.uuid_names {
                            uuid::Uuid::new_v4().to_string().into()
                        } else {
                            TextureHeader::mip_path(&tex_path, mip as u32)
                        }
                    })
                    .collect(),
                format: if compress {
                    texture.usage.into_compressed_format()
                } else {
                    texture.usage.into_format()
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
            let mut f = BufWriter::new(fs::File::create(&header_path).unwrap());
            bincode::serialize_into(&mut f, &tex_header).unwrap();

            // Compute each mip and save to disc
            for mip in 0..mip_count {
                // Resize the image base on the mip count
                let (mut width, mut height) = image.dimensions();
                width = (width >> mip).max(1);
                height = (height >> mip).max(1);

                let downsampled =
                    image.resize(width, height, image::imageops::FilterType::Lanczos3);

                // Convert the image into a byte array
                let mut bytes = downsampled.to_rgba8().to_vec();

                // Compress if requested
                if compress {
                    let surface = intel_tex_2::RgbaSurface {
                        width,
                        height,
                        stride: width * 4,
                        data: &bytes,
                    };
                    bytes = intel_tex_2::bc7::compress_blocks(
                        &intel_tex_2::bc7::alpha_ultra_fast_settings(),
                        &surface,
                    );
                }

                let tex_data = TextureData::new(bytes, width, height, format);

                // Save the file to disk
                let mip_out_path = if args.uuid_names {
                    let mut data_path = PathBuf::from(out);
                    data_path.push(tex_header.mips[mip].clone());
                    AssetNameBuf::from_path_buf(data_path).unwrap()
                } else {
                    tex_header.mips[mip].clone()
                };

                let mut f = BufWriter::new(fs::File::create(mip_out_path).unwrap());
                bincode::serialize_into(&mut f, &tex_data).unwrap();
            }
        });
}

/// Helper to determine if a texture needs compression.
#[inline]
fn texture_needs_compression(image: &image::DynamicImage) -> bool {
    let (width, height) = image.dimensions();

    // We only need to compress if our image is at least as big as a block
    width >= 4 && height >= 4
}

/// Helper to determine how many mips a texture needs.
#[inline]
fn texture_mip_count(image: &image::DynamicImage, compressed: bool) -> usize {
    let (width, height) = image.dimensions();
    if compressed {
        (width.div(4).max(height.div(4)) as f32).log2() as usize + 1
    } else {
        (width.max(height) as f32).log2() as usize + 1
    }
}

/// Helper to compute tangents from UVs and positions
fn compute_tangents(positions: &[Vec4], uvs: &[Vec2], indices: &[u32]) -> Vec<Vec4> {
    assert!(!positions.is_empty());
    assert!(positions.len() == uvs.len());

    let mut tangents = vec![Vec4::Z; positions.len()];

    for tri in indices.chunks_exact(3) {
        let p0 = &positions[tri[0] as usize];
        let p1 = &positions[tri[1] as usize];
        let p2 = &positions[tri[2] as usize];

        let uv0 = &uvs[tri[0] as usize];
        let uv1 = &uvs[tri[1] as usize];
        let uv2 = &uvs[tri[2] as usize];

        let edge1 = *p1 - *p0;
        let edge2 = *p2 - *p0;

        let delta_uv1 = *uv1 - *uv0;
        let delta_uv2 = *uv2 - *uv0;

        let f = 1.0 / ((delta_uv1.x * delta_uv2.y) - (delta_uv2.x * delta_uv1.y));

        let t = Vec4::new(
            f * (delta_uv2.y * edge1.x - delta_uv1.y * edge2.x),
            f * (delta_uv2.y * edge1.y - delta_uv1.y * edge2.y),
            f * (delta_uv2.y * edge1.z - delta_uv1.y * edge2.z),
            0.0,
        )
        .normalize();

        tangents[tri[0] as usize] = t;
        tangents[tri[1] as usize] = t;
        tangents[tri[2] as usize] = t;
    }

    tangents
}
