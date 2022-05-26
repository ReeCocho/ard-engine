use std::{any::TypeId, primitive};

use ard_assets::prelude::*;
use ard_graphics_api::prelude::*;
use ard_graphics_vk::prelude as graphics;
use ard_log::warn;
use async_trait::async_trait;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use ard_math::{Mat4, Vec2, Vec3, Vec4};
use gltf::{Glb, Gltf};

use crate::{prelude::PbrMaterialData, PipelineAsset};

pub struct ModelAsset {
    /// List of all loaded meshes.
    pub mesh_groups: Vec<MeshGroup>,
    /// List of all materials.
    pub materials: Vec<graphics::Material>,
    /// All nodes within the model.
    pub nodes: Vec<Node>,
}

pub struct MeshGroup {
    /// Each mesh is associated with a material index.
    pub meshes: Vec<(graphics::Mesh, usize)>,
}

/// A node within a model. The mesh field is an indiex into the associated vector in the model the
/// node is a part of.
pub struct Node {
    pub mesh_group: usize,
    pub transform: Mat4,
}

pub struct ModelLoader {
    pub(crate) factory: graphics::Factory,
}

/// A meta data file that describes a model.
#[derive(Debug, Serialize, Deserialize)]
struct ModelMeta {
    /// Name of the pipeline to use for PBR materials.
    pub pbr_pipeline: AssetNameBuf,
    /// Path to the model file.
    pub model: AssetNameBuf,
}

impl Asset for ModelAsset {
    const EXTENSION: &'static str = "mdl";

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
        // Read in the meta data
        let meta = package.read_str(asset).await?;
        let meta = match ron::from_str::<ModelMeta>(&meta) {
            Ok(meta) => meta,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Load in the model
        let data = package.read(&meta.model).await?;
        let glb = match Glb::from_slice(&data) {
            Ok(glb) => glb,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Extract the binary component
        let bin = glb.bin.unwrap().into_owned();

        // Load the GLTF section
        let gltf_info = match Gltf::from_slice(&glb.json.into_owned()) {
            Ok(gltf_info) => gltf_info,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        // Load in the pipeline for materials
        let pipeline_handle = assets.load_async::<PipelineAsset>(&meta.pbr_pipeline).await;

        // Parse the model
        let mut model = ModelAsset {
            mesh_groups: Vec::default(),
            materials: Vec::default(),
            nodes: Vec::default(),
        };

        // Create PBR materials
        let pipeline = assets.get::<PipelineAsset>(&pipeline_handle).unwrap();
        for material in gltf_info.materials() {
            let info = material.pbr_metallic_roughness();

            let create_info = MaterialCreateInfo {
                pipeline: pipeline.pipeline.clone(),
            };

            let material = self.factory.create_material(&create_info);
            self.factory.update_material_data(
                &material,
                bytemuck::bytes_of(&PbrMaterialData {
                    base_color: Vec4::from(info.base_color_factor()),
                    metallic: info.metallic_factor(),
                    roughness: info.roughness_factor(),
                }),
            );

            model.materials.push(material);
        }
        std::mem::drop(pipeline);

        // Load in every mesh (mesh group in our lingo)
        for mesh in gltf_info.meshes() {
            let mut mesh_group = MeshGroup {
                meshes: Vec::with_capacity(mesh.primitives().len()),
            };

            // Each primitive is a mesh
            for primitive in mesh.primitives() {
                // Get the material index (or skip if there is none)
                let material = match primitive.material().index() {
                    Some(idx) => idx,
                    None => {
                        warn!(
                            "model `{:?}` has a GLTF primitive which uses a default material",
                            asset
                        );
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
                            // Positions are required to have bounds by the spec
                            let min = {
                                let values = accessor.min().unwrap();
                                let values = values.as_array().unwrap();
                                Vec3::new(
                                    values[0].as_f64().unwrap() as f32,
                                    values[1].as_f64().unwrap() as f32,
                                    values[2].as_f64().unwrap() as f32,
                                )
                            };

                            let max = {
                                let values = accessor.max().unwrap();
                                let values = values.as_array().unwrap();
                                Vec3::new(
                                    values[0].as_f64().unwrap() as f32,
                                    values[1].as_f64().unwrap() as f32,
                                    values[2].as_f64().unwrap() as f32,
                                )
                            };

                            let mut center =
                                (Vec4::from((min, 0.0)) + Vec4::from((max, 0.0))) / 2.0;

                            center.w = (min.x.powi(2) + min.z.powi(2) + min.y.powi(2))
                                .max(max.x.powi(2) + max.z.powi(2) + max.y.powi(2))
                                .sqrt();

                            // create_info.bounds = MeshBounds::Manual(ObjectBounds {
                            //     center,
                            //     half_extents: (Vec4::from((max, 0.0)) - Vec4::from((min, 0.0)))
                            //         / 2.0,
                            // });

                            // Copy data into a buffer
                            positions = accessor_to_vec::<Vec4>(accessor, &bin)?;
                        }
                        gltf::Semantic::Normals => {
                            normals = accessor_to_vec::<Vec4>(accessor, &bin)?
                        }
                        gltf::Semantic::Tangents => {
                            tangents = accessor_to_vec::<Vec4>(accessor, &bin)?
                        }
                        gltf::Semantic::Colors(n) => {
                            if n == 0 {
                                colors = accessor_to_vec::<Vec4>(accessor, &bin)?;
                            }
                        }
                        gltf::Semantic::TexCoords(n) => match n {
                            0 => uv0 = accessor_to_vec::<Vec2>(accessor, &bin)?,
                            1 => uv1 = accessor_to_vec::<Vec2>(accessor, &bin)?,
                            2 => uv2 = accessor_to_vec::<Vec2>(accessor, &bin)?,
                            3 => uv3 = accessor_to_vec::<Vec2>(accessor, &bin)?,
                            _ => continue,
                        },
                        _ => {
                            return Err(AssetLoadError::Other(
                                String::from("weights and joints not supported").into(),
                            ))
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
                        return Err(AssetLoadError::Other(
                            String::from("primitives must have indices in GLTF").into(),
                        ))
                    }
                };

                let indices_view = match indices_accessor.view() {
                    Some(view) => view,
                    None => {
                        return Err(AssetLoadError::Other(
                            String::from("no support for sparse attributes in GLTF").into(),
                        ))
                    }
                };

                match indices_accessor.data_type() {
                    gltf::accessor::DataType::U16 => {
                        // Convert list of u16s into u32s
                        let mut indices = Vec::with_capacity(indices_accessor.count());

                        let read_offset = indices_accessor.offset() + indices_view.offset();
                        let len = indices_accessor.count() * std::mem::size_of::<u16>();

                        let u16_inds =
                            bytemuck::cast_slice::<u8, u16>(&bin[read_offset..(read_offset + len)]);

                        for ind in u16_inds {
                            indices.push(*ind as u32)
                        }

                        create_info.indices = &indices;

                        // Create the mesh
                        mesh_group
                            .meshes
                            .push((self.factory.create_mesh(&create_info), material));
                    }
                    gltf::accessor::DataType::U32 => {
                        let read_offset = indices_accessor.offset() + indices_view.offset();
                        let len = indices_accessor.count() * std::mem::size_of::<u32>();

                        create_info.indices =
                            bytemuck::cast_slice::<u8, u32>(&bin[read_offset..(read_offset + len)]);

                        // Create the mesh
                        mesh_group
                            .meshes
                            .push((self.factory.create_mesh(&create_info), material));
                    }
                    other => {
                        return Err(AssetLoadError::Other(
                            format!("unsupported index type `{:?}` in GLTF", other).into(),
                        ))
                    }
                }
            }

            model.mesh_groups.push(mesh_group);
        }

        // Create nodes
        for node in gltf_info.nodes() {
            // Skip if no mesh group
            let mesh_group = match node.mesh() {
                Some(mesh) => mesh.index(),
                None => {
                    warn!("model `{:?}` has a GLTF node with no mesh", asset);
                    continue;
                }
            };

            // Determine the model matrix
            let model_mat = node.transform().matrix();

            model.nodes.push(Node {
                mesh_group,
                transform: Mat4::from_cols_array_2d(&model_mat),
            });
        }

        Ok(AssetLoadResult::Loaded {
            asset: model,
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

/// Takes an accessor and turns the data referenced into a buffer of another type.
fn accessor_to_vec<T: Pod + Zeroable + 'static>(
    accessor: gltf::Accessor,
    raw: &[u8],
) -> Result<Vec<T>, AssetLoadError> {
    // Don't support non-float data types
    if accessor.data_type() != gltf::accessor::DataType::F32 {
        return Err(AssetLoadError::Other(
            String::from("no support for types other than float in GLTF").into(),
        ));
    }

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
    // to u8. Same thing happens in the return.
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
        gltf::accessor::Dimensions::Scalar => std::mem::size_of::<f32>(),
        gltf::accessor::Dimensions::Vec2 => 2 * std::mem::size_of::<f32>(),
        gltf::accessor::Dimensions::Vec3 => 3 * std::mem::size_of::<f32>(),
        gltf::accessor::Dimensions::Vec4 => 4 * std::mem::size_of::<f32>(),
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
