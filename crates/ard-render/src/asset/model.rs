use ard_assets::prelude::*;
use ard_ecs::prelude::{Entity, EntityCommands};
use ard_log::warn;
use ard_math::{Mat4, Quat, Vec2, Vec3, Vec4};
use ard_pal::prelude::{Filter, SamplerAddressMode, TextureFormat};
use async_trait::async_trait;
use bytemuck::{Pod, Zeroable};
use gltf::{Glb, Gltf};
use image::GenericImageView;
use serde::{Deserialize, Serialize};

use crate::{
    factory::Factory,
    material::{Material, MaterialInstance, MaterialInstanceCreateInfo},
    mesh::{Mesh, MeshBounds, MeshCreateInfo},
    pbr::{
        PbrMaterial, PBR_DIFFUSE_MAP_SLOT, PBR_METALLIC_ROUGHNESS_MAP_SLOT, PBR_NORMAL_MAP_SLOT,
    },
    renderer::{Model, RenderLayer, Renderable},
    static_geometry::{StaticGeometry, StaticRenderable, StaticRenderableHandle},
    texture::{MipType, Sampler, Texture, TextureCreateInfo},
};

pub struct ModelAsset {
    pub textures: Vec<Texture>,
    pub materials: Vec<MaterialInstance>,
    pub mesh_groups: Vec<MeshGroup>,
    pub roots: Vec<Node>,
    pub node_count: usize,
}

pub struct MeshGroup {
    /// Each mesh is associated with a material index.
    pub meshes: Vec<(Mesh, usize)>,
}

pub struct Node {
    /// The name of this node.
    pub name: String,
    /// Model matrix for this node in local space.
    pub model: Mat4,
    /// Index of the mesh group for this node.
    pub mesh_group: Option<usize>,
    /// All child nodes of this node.
    pub children: Vec<Node>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Path to the model file.
    pub model: AssetNameBuf,
}

pub struct ModelLoader {
    factory: Factory,
    pbr_material: Material,
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

        // Load in the model
        let data = package.read(&desc.model).await?;
        let glb = match Glb::from_slice(&data) {
            Ok(glb) => glb,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Extract the binary component
        let bin = glb.bin.unwrap().into_owned();

        // Load the GLTF section
        let gltf_info = match Gltf::from_slice(&glb.json.into_owned()) {
            Ok(gltf_info) => gltf_info,
            Err(err) => return Err(AssetLoadError::Other(err.to_string())),
        };

        // Load textures
        let textures = load_gltf_textures(&self.factory, &gltf_info, &bin)?;

        // Load materials and mesh groups at the same time
        let (materials, mesh_groups) = rayon::join(
            || load_gltf_materials(&self.factory, &gltf_info, &textures, &self.pbr_material),
            || load_gltf_mesh_groups(&self.factory, &gltf_info, &bin),
        );

        let materials = materials?;
        let mesh_groups = mesh_groups?;

        // Load in the root nodes of every scene
        let gltf_info = gltf_info.document.into_json();

        // Load all the nodes recursively
        let mut roots = Vec::default();
        for scene in &gltf_info.scenes {
            for node in &scene.nodes {
                roots.push(load_gltf_node(node.value(), &gltf_info.nodes))
            }
        }

        Ok(AssetLoadResult::Loaded {
            asset: ModelAsset {
                textures,
                materials,
                mesh_groups,
                roots,
                node_count: gltf_info.nodes.len(),
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

impl ModelAsset {
    pub fn instantiate_dyn(&self, commands: &EntityCommands) -> Vec<Entity> {
        let mut components = (
            // Models
            Vec::with_capacity(self.node_count),
            // Renderables
            Vec::with_capacity(self.node_count),
        );

        fn traverse(
            parent_model: Mat4,
            node: &Node,
            asset: &ModelAsset,
            components: &mut (Vec<Model>, Vec<Renderable>),
        ) {
            if let Some(mesh_group_idx) = node.mesh_group {
                let mesh_group = &asset.mesh_groups[mesh_group_idx];
                for (mesh, material_idx) in &mesh_group.meshes {
                    let material = &asset.materials[*material_idx];
                    components.0.push(Model(parent_model * node.model));
                    components.1.push(Renderable {
                        mesh: mesh.clone(),
                        material: material.clone(),
                        layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                    });
                }
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut components);
        }

        let mut entities = Vec::with_capacity(components.0.len());
        entities.resize(components.0.len(), Entity::null());
        commands.create(components, &mut entities);

        entities
    }

    pub fn instantiate_static(&self, static_geo: &StaticGeometry) -> Vec<StaticRenderableHandle> {
        let mut renderables = Vec::with_capacity(self.node_count);

        fn traverse(
            parent_model: Mat4,
            node: &Node,
            asset: &ModelAsset,
            renderables: &mut Vec<StaticRenderable>,
        ) {
            if let Some(mesh_group_idx) = node.mesh_group {
                let mesh_group = &asset.mesh_groups[mesh_group_idx];
                for (mesh, material_idx) in &mesh_group.meshes {
                    let material = &asset.materials[*material_idx];
                    renderables.push(StaticRenderable {
                        renderable: Renderable {
                            mesh: mesh.clone(),
                            material: material.clone(),
                            layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                        },
                        model: Model(parent_model * node.model),
                        entity: Entity::null(),
                    });
                }
            }

            for child in &node.children {
                traverse(node.model, child, asset, renderables);
            }
        }

        for root in &self.roots {
            traverse(Mat4::IDENTITY, root, self, &mut renderables);
        }

        static_geo.register(&renderables)
    }
}

fn load_gltf_textures(
    factory: &Factory,
    gltf: &Gltf,
    bin: &[u8],
) -> Result<Vec<Texture>, AssetLoadError> {
    use rayon::prelude::*;

    let mut results: Vec<(usize, Result<Texture, AssetLoadError>)> = gltf
        .textures()
        .enumerate()
        .par_bridge()
        .into_par_iter()
        .map(|(i, texture)| {
            // Graph the buffer view and type of image
            let (view, mime_type) = match texture.source().source() {
                gltf::image::Source::View { view, mime_type } => (view, mime_type),
                gltf::image::Source::Uri { .. } => {
                    return (
                        i,
                        Err(AssetLoadError::Other(
                            "unable to use uri as texture source in GLTF".into(),
                        )),
                    )
                }
            };

            // Determine the image codec
            let codec = match mime_type {
                "image/jpeg" => image::ImageFormat::Jpeg,
                "image/png" => image::ImageFormat::Png,
                unknown => {
                    return (
                        i,
                        Err(AssetLoadError::Other(
                            format!("unknown texture format `{}` in GLTF", unknown).into(),
                        )),
                    )
                }
            };

            // Load the image and convert it to SRGB
            let image = match view.stride() {
                // Stride sucks :(
                // We have to construct a temporary buffer to house the texture data
                Some(_) => {
                    return (
                        i,
                        Err(AssetLoadError::Other(
                            String::from("stride detected in image").into(),
                        )),
                    )
                }
                // No stride is great. We can directly reference the image data in bin
                None => match image::load_from_memory_with_format(
                    &bin[view.offset()..(view.offset() + view.length())],
                    codec,
                ) {
                    Ok(image) => image,
                    Err(err) => return (i, Err(AssetLoadError::Other(err.to_string()))),
                },
            };

            let (width, height) = image.dimensions();
            let image = image.resize(width / 4, height / 4, image::imageops::FilterType::Nearest);
            let raw = image.to_rgba8();

            let max = gltf_to_pal_mag_filter(
                texture
                    .sampler()
                    .mag_filter()
                    .unwrap_or(gltf::texture::MagFilter::Linear),
            );
            let (min, mip) = gltf_to_pal_min_filter(
                texture
                    .sampler()
                    .min_filter()
                    .unwrap_or(gltf::texture::MinFilter::Linear),
            );
            let wrap_u = gltf_to_pal_wrap_mode(texture.sampler().wrap_s());
            let wrap_v = gltf_to_pal_wrap_mode(texture.sampler().wrap_t());

            // Create the texture
            let create_info = TextureCreateInfo {
                width: image.width(),
                height: image.height(),
                format: TextureFormat::Rgba8Srgb,
                data: &raw,
                mip_type: if mip.is_some() {
                    MipType::Generate
                } else {
                    MipType::Upload
                },
                mip_count: if mip.is_some() {
                    (image.width().max(image.height()) as f32).log2() as usize + 1
                } else {
                    1
                },
                sampler: Sampler {
                    min_filter: min,
                    mag_filter: max,
                    mipmap_filter: mip.unwrap_or(Filter::Linear),
                    address_u: wrap_u,
                    address_v: wrap_v,
                    anisotropy: true,
                },
            };

            (i, Ok(factory.create_texture(create_info)))
        })
        .collect();

    // Par-bridge does not gaurantee ordering, so we must do it ourselves
    results.sort_unstable_by_key(|(i, _)| *i);

    // Check if any are err
    let mut textures = Vec::with_capacity(results.len());
    for (_, res) in results {
        match res {
            Ok(texture) => textures.push(texture),
            Err(err) => return Err(err),
        }
    }

    Ok(textures)
}

fn load_gltf_materials(
    factory: &Factory,
    gltf: &Gltf,
    textures: &[Texture],
    pbr_material: &Material,
) -> Result<Vec<MaterialInstance>, AssetLoadError> {
    use rayon::prelude::*;

    let mut materials: Vec<(usize, MaterialInstance)> = gltf
        .materials()
        .enumerate()
        .par_bridge()
        .into_par_iter()
        .map(|(i, material)| {
            // Create the material instance
            let info = material.pbr_metallic_roughness();
            let create_info = MaterialInstanceCreateInfo {
                material: pbr_material.clone(),
            };
            let material_instance = factory.create_material_instance(create_info);

            // Write in the material data
            factory.update_material_data(
                &material_instance,
                &PbrMaterial {
                    base_color: Vec4::from(info.base_color_factor()),
                    metallic: info.metallic_factor(),
                    roughness: info.roughness_factor(),
                },
            );

            // Write in textures
            if let Some(tex) = info.base_color_texture() {
                factory.update_material_texture(
                    &material_instance,
                    Some(&textures[tex.texture().index()]),
                    PBR_DIFFUSE_MAP_SLOT,
                );
            }

            if let Some(tex) = info.metallic_roughness_texture() {
                factory.update_material_texture(
                    &material_instance,
                    Some(&textures[tex.texture().index()]),
                    PBR_METALLIC_ROUGHNESS_MAP_SLOT,
                );
            }

            if let Some(tex) = material.normal_texture() {
                factory.update_material_texture(
                    &material_instance,
                    Some(&textures[tex.texture().index()]),
                    PBR_NORMAL_MAP_SLOT,
                );
            }

            (i, material_instance)
        })
        .collect();

    // Par-bridge does not gaurantee ordering, so we must do it ourselves
    materials.sort_unstable_by_key(|(i, _)| *i);

    Ok(materials
        .into_iter()
        .map(|(_, material)| material)
        .collect())
}

fn load_gltf_mesh_groups(
    factory: &Factory,
    gltf: &Gltf,
    bin: &[u8],
) -> Result<Vec<MeshGroup>, AssetLoadError> {
    use rayon::prelude::*;

    let mut results: Vec<(usize, Result<MeshGroup, AssetLoadError>)> = gltf
        .meshes()
        .enumerate()
        .par_bridge()
        .into_par_iter()
        .map(|(i, mesh)| {
            let mut mesh_group = MeshGroup {
                meshes: Vec::default(),
            };

            for primitive in mesh.primitives() {
                let material = match primitive.material().index() {
                    Some(idx) => idx,
                    None => {
                        warn!("model has a GLTF primtiive with a default material");
                        continue;
                    }
                };

                // Container for temporary buffers
                let mut positions = Vec::default();
                let mut normals = Vec::default();
                let mut tangents = Vec::default();
                let mut colors = Vec::default();
                let mut uv0 = Vec::default();
                let mut uv1 = Vec::default();
                let mut uv2 = Vec::default();
                let mut uv3 = Vec::default();

                let mut create_info = MeshCreateInfo {
                    bounds: MeshBounds::Generate,
                    indices: &[],
                    positions: &[],
                    normals: None,
                    tangents: None,
                    colors: None,
                    uv0: None,
                    uv1: None,
                    uv2: None,
                    uv3: None,
                };

                // Load in all attributes
                for (semantic, accessor) in primitive.attributes() {
                    match semantic {
                        gltf::Semantic::Positions => {
                            // Copy data into a buffer
                            positions = match accessor_to_vec::<Vec4>(
                                accessor,
                                &bin,
                                gltf::accessor::DataType::F32,
                            ) {
                                Ok(res) => res,
                                Err(err) => return (i, Err(err)),
                            };
                        }
                        gltf::Semantic::Normals => {
                            normals = match accessor_to_vec::<Vec4>(
                                accessor,
                                &bin,
                                gltf::accessor::DataType::F32,
                            ) {
                                Ok(res) => res,
                                Err(err) => return (i, Err(err)),
                            };
                        }
                        gltf::Semantic::Tangents => {
                            tangents = match accessor_to_vec::<Vec4>(
                                accessor,
                                &bin,
                                gltf::accessor::DataType::F32,
                            ) {
                                Ok(res) => res,
                                Err(err) => return (i, Err(err)),
                            };
                        }
                        gltf::Semantic::Colors(n) => {
                            if n == 0 {
                                colors = match accessor_to_vec::<Vec4>(
                                    accessor,
                                    &bin,
                                    gltf::accessor::DataType::F32,
                                ) {
                                    Ok(res) => res,
                                    Err(err) => return (i, Err(err)),
                                };
                            }
                        }
                        gltf::Semantic::TexCoords(n) => match n {
                            0 => {
                                uv0 = match accessor_to_vec::<Vec2>(
                                    accessor,
                                    &bin,
                                    gltf::accessor::DataType::F32,
                                ) {
                                    Ok(res) => res,
                                    Err(err) => return (i, Err(err)),
                                };
                            }
                            1 => {
                                uv1 = match accessor_to_vec::<Vec2>(
                                    accessor,
                                    &bin,
                                    gltf::accessor::DataType::F32,
                                ) {
                                    Ok(res) => res,
                                    Err(err) => return (i, Err(err)),
                                };
                            }
                            2 => {
                                uv2 = match accessor_to_vec::<Vec2>(
                                    accessor,
                                    &bin,
                                    gltf::accessor::DataType::F32,
                                ) {
                                    Ok(res) => res,
                                    Err(err) => return (i, Err(err)),
                                };
                            }
                            3 => {
                                uv3 = match accessor_to_vec::<Vec2>(
                                    accessor,
                                    &bin,
                                    gltf::accessor::DataType::F32,
                                ) {
                                    Ok(res) => res,
                                    Err(err) => return (i, Err(err)),
                                };
                            }
                            _ => continue,
                        },
                        _ => {
                            return (
                                i,
                                Err(AssetLoadError::Other(
                                    String::from("weights and joints not supported").into(),
                                )),
                            )
                        }
                    }
                }

                // Set attributes
                create_info.positions = &positions;

                if !normals.is_empty() {
                    create_info.normals = Some(&normals);
                }

                if !tangents.is_empty() {
                    create_info.tangents = Some(&tangents);
                }

                if !colors.is_empty() {
                    create_info.colors = Some(&colors);
                }

                if !uv0.is_empty() {
                    create_info.uv0 = Some(&uv0);
                }

                if !uv1.is_empty() {
                    create_info.uv1 = Some(&uv1);
                }

                if !uv2.is_empty() {
                    create_info.uv2 = Some(&uv2);
                }

                if !uv3.is_empty() {
                    create_info.uv3 = Some(&uv3);
                }

                // Load in the indices. They are required to be u32 by the GLTF spec
                let indices_accessor = match primitive.indices() {
                    Some(accessor) => accessor,
                    None => {
                        return (
                            i,
                            Err(AssetLoadError::Other(
                                String::from("primitives must have indices in GLTF").into(),
                            )),
                        )
                    }
                };

                match indices_accessor.data_type() {
                    gltf::accessor::DataType::U16 => {
                        let indices = match accessor_to_vec::<u16>(
                            indices_accessor,
                            &bin,
                            gltf::accessor::DataType::U16,
                        ) {
                            Ok(res) => res,
                            Err(err) => return (i, Err(err)),
                        };
                        let mut as_u32 = Vec::with_capacity(indices.len());
                        for i in indices {
                            as_u32.push(i as u32);
                        }

                        create_info.indices = &as_u32;

                        mesh_group
                            .meshes
                            .push((factory.create_mesh(create_info), material));
                    }
                    gltf::accessor::DataType::U32 => {
                        let indices = match accessor_to_vec::<u32>(
                            indices_accessor,
                            &bin,
                            gltf::accessor::DataType::U32,
                        ) {
                            Ok(res) => res,
                            Err(err) => return (i, Err(err)),
                        };
                        create_info.indices = &indices;

                        mesh_group
                            .meshes
                            .push((factory.create_mesh(create_info), material));
                    }
                    other => {
                        return (
                            i,
                            Err(AssetLoadError::Other(
                                format!("unsupported index type `{:?}` in GLTF", other).into(),
                            )),
                        )
                    }
                }
            }

            (i, Ok(mesh_group))
        })
        .collect();

    // Par-bridge does not gaurantee ordering, so we must do it ourselves
    results.sort_unstable_by_key(|(i, _)| *i);

    // Check if any are err
    let mut mesh_groups = Vec::with_capacity(results.len());
    for (_, res) in results {
        match res {
            Ok(mesh_group) => mesh_groups.push(mesh_group),
            Err(err) => return Err(err),
        }
    }

    Ok(mesh_groups)
}

fn load_gltf_node(node_idx: usize, all_nodes: &[gltf::json::Node]) -> Node {
    let node = &all_nodes[node_idx];

    // Either construct the model matrix or grab it from the file
    let model = match &node.matrix {
        Some(model) => Mat4::from_cols_array(model),
        None => {
            let translate = Vec3::from_slice(&node.translation.unwrap_or_default());
            let rotate = Quat::from_array(node.rotation.unwrap_or_default().0);
            let scale = Vec3::from_slice(&node.scale.unwrap_or([1.0; 3]));

            let mut model = Mat4::from_scale(scale);
            model = Mat4::from_quat(rotate) * model;
            model = Mat4::from_translation(translate) * model;

            model
        }
    };

    // Create the node
    let mut out_node = Node {
        name: node
            .name
            .as_ref()
            .map(|n| n.as_str())
            .unwrap_or("")
            .to_string(),
        model,
        mesh_group: node.mesh.map(|m| m.value()),
        children: Vec::default(),
    };

    // Load in children
    if let Some(children) = &node.children {
        for child in children {
            out_node
                .children
                .push(load_gltf_node(child.value(), all_nodes));
        }
    }

    out_node
}

/// Takes an accessor and turns the data referenced into a buffer of another type.
fn accessor_to_vec<T: Pod + Zeroable + 'static>(
    accessor: gltf::Accessor,
    raw: &[u8],
    expected_data_type: gltf::accessor::DataType,
) -> Result<Vec<T>, AssetLoadError> {
    // Don't support non-float data types
    if accessor.data_type() != expected_data_type {
        return Err(AssetLoadError::Other(
            format!(
                "expected `{:?}` accessor data type but got `{:?}` in GLTF",
                expected_data_type,
                accessor.data_type()
            )
            .into(),
        ));
    }

    let data_size = match expected_data_type {
        gltf::accessor::DataType::I8 => std::mem::size_of::<i8>(),
        gltf::accessor::DataType::U8 => std::mem::size_of::<u8>(),
        gltf::accessor::DataType::I16 => std::mem::size_of::<i16>(),
        gltf::accessor::DataType::U16 => std::mem::size_of::<u16>(),
        gltf::accessor::DataType::U32 => std::mem::size_of::<u32>(),
        gltf::accessor::DataType::F32 => std::mem::size_of::<f32>(),
    };

    // Must have a view
    let view = match accessor.view() {
        Some(view) => view,
        None => {
            return Err(AssetLoadError::Other(
                String::from("no support for sparse attributes in GLTF").into(),
            ))
        }
    };

    // Ensure the buffer is from the binary blob and not a uri
    if let gltf::buffer::Source::Uri(_) = view.buffer().source() {
        return Err(AssetLoadError::Other(
            String::from("no support for vertex data from URI").into(),
        ));
    }

    // Create a raw buffer for the point data
    // NOTE: We have to use unsafe here because bytemuck requires the alignments to be the same.
    // The u8 alignment requirement is less strict than T, so we initialize as T and then convert
    // to u8. Same thing but in reverse happens in the return.
    let mut points = unsafe {
        let mut buf = Vec::<T>::with_capacity(accessor.count());
        let ptr = buf.as_mut_ptr();
        let cap = accessor.count();
        std::mem::forget(buf);
        Vec::<u8>::from_raw_parts(ptr as *mut u8, 0, cap * std::mem::size_of::<T>())
    };
    points.resize(accessor.count() * std::mem::size_of::<T>(), 0);

    // Determine strides and sizes for copying data
    let read_size = match accessor.dimensions() {
        gltf::accessor::Dimensions::Scalar => data_size,
        gltf::accessor::Dimensions::Vec2 => 2 * data_size,
        gltf::accessor::Dimensions::Vec3 => 3 * data_size,
        gltf::accessor::Dimensions::Vec4 => 4 * data_size,
        _ => {
            return Err(AssetLoadError::Other(
                String::from("no support for matrix types in GLTF").into(),
            ))
        }
    };

    let read_stride = match view.stride() {
        Some(stride) => stride,
        None => read_size,
    };

    let write_size = std::mem::size_of::<T>();

    // Read size has to be less than or equal to the write size, otherwise we are copying OOB
    if read_size > write_size {
        return Err(AssetLoadError::Other(
            String::from("vertex attribute is bigger than requested type in GLTF").into(),
        ));
    }

    let mut read_offset = accessor.offset() + view.offset();
    let mut write_offset = 0;

    // If our read stride and write sizes are equal, we're lucky. We can just do a straight memcpy
    if read_stride == write_size {
        let len = points.len();
        points.copy_from_slice(&raw[read_offset..(read_offset + len)]);
    }
    // Otherwise, data is probably interleaved so we have to do a bunch of copies
    else {
        while write_offset != points.len() {
            points[write_offset..(write_offset + read_size)]
                .copy_from_slice(&raw[read_offset..(read_offset + read_size)]);
            read_offset += read_stride;
            write_offset += write_size;
        }
    }

    unsafe {
        let ptr = points.as_mut_ptr();
        let cap = points.capacity();
        let len = points.len();
        std::mem::forget(points);
        Ok(Vec::<T>::from_raw_parts(
            ptr as *mut T,
            len / std::mem::size_of::<T>(),
            cap / std::mem::size_of::<T>(),
        ))
    }
}

/// First filter is the texture filter. Second is for mip maps. If second is `None`, mip maps
/// should not be generated.
#[inline(always)]
const fn gltf_to_pal_min_filter(filter: gltf::texture::MinFilter) -> (Filter, Option<Filter>) {
    match filter {
        gltf::texture::MinFilter::Nearest => (Filter::Nearest, None),
        gltf::texture::MinFilter::Linear => (Filter::Linear, None),
        gltf::texture::MinFilter::NearestMipmapNearest => (Filter::Nearest, Some(Filter::Nearest)),
        gltf::texture::MinFilter::LinearMipmapNearest => (Filter::Linear, Some(Filter::Nearest)),
        gltf::texture::MinFilter::NearestMipmapLinear => (Filter::Nearest, Some(Filter::Linear)),
        gltf::texture::MinFilter::LinearMipmapLinear => (Filter::Linear, Some(Filter::Linear)),
    }
}

#[inline(always)]
const fn gltf_to_pal_mag_filter(filter: gltf::texture::MagFilter) -> Filter {
    match filter {
        gltf::texture::MagFilter::Nearest => Filter::Nearest,
        gltf::texture::MagFilter::Linear => Filter::Linear,
    }
}

#[inline(always)]
const fn gltf_to_pal_wrap_mode(mode: gltf::texture::WrappingMode) -> SamplerAddressMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => SamplerAddressMode::ClampToEdge,
        gltf::texture::WrappingMode::MirroredRepeat => SamplerAddressMode::MirroredRepeat,
        gltf::texture::WrappingMode::Repeat => SamplerAddressMode::Repeat,
    }
}
