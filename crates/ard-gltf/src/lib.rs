use std::collections::HashMap;

use ard_math::{Mat4, Quat, Vec2, Vec3, Vec4};
use ard_pal::prelude::{Filter, Format, SamplerAddressMode};
use bytemuck::{Pod, Zeroable};
use gltf::{json::extensions::scene::khr_lights_punctual, Glb, Gltf};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use thiserror::Error;

pub struct GltfModel {
    pub lights: Vec<GltfLight>,
    pub textures: Vec<GltfTexture>,
    pub materials: Vec<GltfMaterial>,
    pub mesh_groups: Vec<GltfMeshGroup>,
    pub meshes: Vec<GltfMesh>,
    pub roots: Vec<GltfNode>,
}

#[derive(Debug, Error)]
pub enum GltfModelParseError {
    #[error("glb parsing error")]
    ParseError,
}

pub enum GltfMaterial {
    Pbr {
        base_color: Vec4,
        metallic: f32,
        roughness: f32,
        alpha_cutoff: f32,
        diffuse_map: Option<usize>,
        normal_map: Option<usize>,
        metallic_roughness_map: Option<usize>,
        blending: BlendType,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlendType {
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureSourceFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureUsage {
    /// Texture is used as a diffuse color map.
    ///
    /// This maps to the `TextureFormat::Rgba8Srgb` format.
    Diffuse,
    /// Texture is used as a normal map.
    ///
    /// This maps to the `TextureFormat::Rgba8Unorm` format.
    Normal,
    /// Texture is used as a combined metallic roughness map.
    ///
    /// This maps to the `TextureFormat::Rg8Unorm` format.
    MetallicRoughness,
}

pub struct GltfTexture {
    /// Raw image data.
    pub data: Vec<u8>,
    /// How to interpret the image data.
    pub src_format: TextureSourceFormat,
    /// What this image is used for in the model.
    pub usage: TextureUsage,
    /// How this texture should be sampled.
    pub sampler: GltfSampler,
    /// If this texture needs mip maps.
    pub mips: bool,
}

pub struct GltfSampler {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mipmap_filter: Filter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
}

#[derive(Default)]
pub struct GltfMesh {
    pub indices: Vec<u32>,
    pub positions: Vec<Vec4>,
    pub normals: Option<Vec<Vec4>>,
    pub tangents: Option<Vec<Vec4>>,
    pub colors: Option<Vec<Vec4>>,
    pub uv0: Option<Vec<Vec2>>,
    pub uv1: Option<Vec<Vec2>>,
    pub uv2: Option<Vec<Vec2>>,
    pub uv3: Option<Vec<Vec2>>,
}

pub struct GltfMeshInstance {
    pub mesh: usize,
    pub material: usize,
}

pub struct GltfMeshGroup(pub Vec<GltfMeshInstance>);

pub enum GltfLight {
    Point {
        color: Vec3,
        intensity: f32,
        range: f32,
    },
    Spot {
        color: Vec3,
        intensity: f32,
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    },
    Directional {
        color: Vec3,
        intensity: f32,
    },
}

pub struct GltfNode {
    /// The name of this node.
    pub name: String,
    /// Model matrix for this node in local space.
    pub model: Mat4,
    /// Data contained within this node.
    pub data: GltfNodeData,
    /// All child nodes of this node.
    pub children: Vec<GltfNode>,
}

pub enum GltfNodeData {
    Empty,
    MeshGroup(usize),
    Light(usize),
}

#[derive(Clone, Default)]
struct DataMapping {
    lights: HashMap<usize, usize>,
    mesh_groups: HashMap<usize, usize>,
    textures: HashMap<usize, (usize, TextureUsage)>,
    materials: HashMap<usize, usize>,
}

#[derive(Clone, Default)]
struct InvDataMapping {
    lights: HashMap<usize, usize>,
    mesh_groups: HashMap<usize, usize>,
    /// Maps accessor index to associated primitives.
    meshes: HashMap<Accessor, Vec<Primitive>>,
    mesh_count: usize,
    textures: HashMap<usize, (usize, TextureUsage)>,
    materials: HashMap<usize, usize>,
}

// It would definetly be a good idea to pack these values into an array that we can key by
// converting the accessor type to an index. This would simplify a lot of the associated
// primitive code.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Primitive {
    mesh_idx: usize,
    indices: Accessor,
    positions: Accessor,
    normals: Option<Accessor>,
    tangents: Option<Accessor>,
    colors: Option<Accessor>,
    uv0s: Option<Accessor>,
    uv1s: Option<Accessor>,
    uv2s: Option<Accessor>,
    uv3s: Option<Accessor>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Accessor {
    buffer: usize,
    component_type: u32,
    component_count: u32,
    count: u32,
    byte_offset: u32,
    byte_stride: Option<u32>,
}

impl GltfModel {
    pub fn from_slice(data: &[u8]) -> Result<Self, GltfModelParseError> {
        // Load as GLB
        let glb = Glb::from_slice(data)?;
        let bin = glb.bin.unwrap().into_owned();
        let gltf = Gltf::from_slice(&glb.json)?;

        // Mappings from GLTF item indices to our own internal ones
        let mut inv_mapping = InvDataMapping::default();

        // Determine what resources are actually used and also construct the scene graph
        let gltf_doc = gltf.document.into_json();
        let mut roots = Vec::default();
        for scene in &gltf_doc.scenes {
            for node in &scene.nodes {
                roots.push(parse_node(node.value(), &gltf_doc, &mut inv_mapping));
            }
        }

        // Clone and remap from gltf indices -> our indices to our indices -> gltf indices
        let mut mapping = DataMapping::default();
        mapping.lights = inv_mapping.lights.iter().map(|(i, j)| (*j, *i)).collect();
        mapping.mesh_groups = inv_mapping
            .mesh_groups
            .iter()
            .map(|(i, j)| (*j, *i))
            .collect();
        mapping.textures = inv_mapping
            .textures
            .iter()
            .map(|(i, (j, u))| (*j, (*i, *u)))
            .collect();
        mapping.materials = inv_mapping
            .materials
            .iter()
            .map(|(i, j)| (*j, *i))
            .collect();

        // Construct all resources
        let (lights, (textures, (materials, (meshes, mesh_groups)))) = rayon::join(
            || load_gltf_lights(&gltf_doc, &mapping),
            || {
                rayon::join(
                    || load_gltf_textures(&gltf_doc, &mapping, &bin),
                    || {
                        rayon::join(
                            || load_gltf_materials(&gltf_doc, &mapping, &inv_mapping),
                            || {
                                rayon::join(
                                    || load_gltf_meshes(&gltf_doc, &inv_mapping, &bin),
                                    || load_gltf_mesh_groups(&gltf_doc, &mapping, &inv_mapping),
                                )
                            },
                        )
                    },
                )
            },
        );

        Ok(GltfModel {
            lights,
            textures,
            materials,
            mesh_groups,
            meshes,
            roots,
        })
    }
}

impl TextureUsage {
    #[inline]
    pub fn into_format(self) -> Format {
        match self {
            TextureUsage::Diffuse => Format::Rgba8Srgb,
            TextureUsage::Normal => Format::Rgba8Unorm,
            TextureUsage::MetallicRoughness => Format::Rg8Unorm,
        }
    }

    #[inline]
    pub fn into_compressed_format(self) -> Format {
        match self {
            TextureUsage::Diffuse => Format::BC7Srgb,
            TextureUsage::Normal => Format::BC7Unorm,
            TextureUsage::MetallicRoughness => Format::BC7Unorm,
        }
    }
}

impl From<gltf::Error> for GltfModelParseError {
    fn from(_: gltf::Error) -> Self {
        GltfModelParseError::ParseError
    }
}

impl Default for GltfSampler {
    fn default() -> Self {
        GltfSampler {
            min_filter: Filter::Linear,
            mag_filter: Filter::Linear,
            mipmap_filter: Filter::Linear,
            address_u: SamplerAddressMode::ClampToEdge,
            address_v: SamplerAddressMode::ClampToEdge,
        }
    }
}

impl Default for Accessor {
    fn default() -> Self {
        Accessor {
            buffer: usize::MAX,
            component_type: u32::MAX,
            component_count: u32::MAX,
            count: u32::MAX,
            byte_offset: u32::MAX,
            byte_stride: None,
        }
    }
}

impl Primitive {
    fn is_subset_of(&self, other: &Primitive) -> bool {
        if self.indices != other.indices || self.positions != other.positions {
            return false;
        }

        if !match (self.normals, other.normals) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.tangents, other.tangents) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.colors, other.colors) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.uv0s, other.uv0s) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.uv1s, other.uv1s) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.uv2s, other.uv2s) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        if !match (self.uv3s, other.uv3s) {
            (Some(l), Some(r)) => l == r,
            (None, Some(_)) | (None, None) => true,
            (Some(_), None) => false,
        } {
            return false;
        }

        true
    }

    /// Compare this primitive with another and does the following.
    ///
    /// - If the two primitives are exactly equal, does nothing and returns `false`.
    /// - If the two primitives have any mismatching accessors, does nothing and returns `true`.
    /// - If this primitive is a subset of the other primitive, does nothing and returns `false`.
    /// - If this primtive is a superset of the other primitive, updates the other primitive with
    ///   this primitives accessors and returns `false`.
    fn compare_and_update_primitive(&mut self, other: &mut Primitive) -> bool {
        // Check if they are exactly equal
        let indices_comp = self.indices == other.indices;
        let positions_comp = self.positions == other.positions;

        if indices_comp
            && positions_comp
            && self.normals == other.normals
            && self.tangents == other.tangents
            && self.colors == other.colors
            && self.uv0s == other.uv0s
            && self.uv1s == other.uv1s
            && self.uv2s == other.uv2s
            && self.uv3s == other.uv3s
        {
            return false;
        }

        #[derive(Copy, Clone, PartialEq, Eq)]
        enum AccessorComp {
            // Accessors are unequal (both are some but with mismatching values).
            Unequal,
            // Accessors are exactly equal (both none or both are some with matching value)
            Equal,
            // We have some while the other has none.
            WeHaveSome,
            // The other has some while we have none.
            TheyHaveSome,
        }

        fn compare_accessors(us: &Option<Accessor>, other: &Option<Accessor>) -> AccessorComp {
            match (us, other) {
                // Both have accessors, but are only equal if the accessors are equal
                (Some(us), Some(other)) => {
                    if us == other {
                        AccessorComp::Equal
                    } else {
                        AccessorComp::Unequal
                    }
                }
                (None, None) => AccessorComp::Equal,
                (Some(_), None) => AccessorComp::WeHaveSome,
                (None, Some(_)) => AccessorComp::TheyHaveSome,
            }
        }

        // Compare indices and positions first since they are non-optional
        if !indices_comp || !positions_comp {
            return true;
        }

        // Now compare optional accessors
        let normals = compare_accessors(&self.normals, &other.normals);
        let tangents = compare_accessors(&self.tangents, &other.tangents);
        let colors = compare_accessors(&self.colors, &other.colors);
        let uv0s = compare_accessors(&self.uv0s, &other.uv0s);
        let uv1s = compare_accessors(&self.uv1s, &other.uv1s);
        let uv2s = compare_accessors(&self.uv2s, &other.uv2s);
        let uv3s = compare_accessors(&self.uv3s, &other.uv3s);

        // Check if we are exactly equal
        if normals == AccessorComp::Equal
            && tangents == AccessorComp::Equal
            && colors == AccessorComp::Equal
            && uv0s == AccessorComp::Equal
            && uv1s == AccessorComp::Equal
            && uv2s == AccessorComp::Equal
            && uv3s == AccessorComp::Equal
        {
            return false;
        }

        // Check if we have any mismatching accessors
        if normals == AccessorComp::Unequal
            || tangents == AccessorComp::Unequal
            || colors == AccessorComp::Unequal
            || uv0s == AccessorComp::Unequal
            || uv1s == AccessorComp::Unequal
            || uv2s == AccessorComp::Unequal
            || uv3s == AccessorComp::Unequal
        {
            return true;
        }

        // At this point, we know that we can take the union of both primitives and come out with
        // "the same" mesh but better (more specialized accessors).
        if normals == AccessorComp::WeHaveSome {
            other.normals = self.normals;
        }

        if tangents == AccessorComp::WeHaveSome {
            other.tangents = self.tangents;
        }

        if colors == AccessorComp::WeHaveSome {
            other.colors = self.colors;
        }

        if uv0s == AccessorComp::WeHaveSome {
            other.uv0s = self.uv0s;
        }

        if uv1s == AccessorComp::WeHaveSome {
            other.uv1s = self.uv1s;
        }

        if uv2s == AccessorComp::WeHaveSome {
            other.uv2s = self.uv2s;
        }

        if uv3s == AccessorComp::WeHaveSome {
            other.uv3s = self.uv3s;
        }

        false
    }
}

fn parse_node(node_idx: usize, gltf: &gltf::json::Root, mapping: &mut InvDataMapping) -> GltfNode {
    let node = &gltf.nodes[node_idx];

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
    let mut out_node = GltfNode {
        name: node.name.as_deref().unwrap_or("").to_string(),
        model,
        data: if let Some(mesh) = node.mesh {
            // Check if this mesh group has been inspected before
            let new_idx = mapping.mesh_groups.len();
            let index = *mapping.mesh_groups.entry(mesh.value()).or_insert_with(|| {
                inspect_mesh_group(
                    mesh.value(),
                    gltf,
                    &mut mapping.textures,
                    &mut mapping.materials,
                    &mut mapping.meshes,
                    &mut mapping.mesh_count,
                );
                new_idx
            });

            GltfNodeData::MeshGroup(index)
        } else if let Some(ext) = &node.extensions {
            if let Some(light) = &ext.khr_lights_punctual {
                let new_idx = mapping.lights.len();
                let index = *mapping.lights.entry(light.light.value()).or_insert(new_idx);

                GltfNodeData::Light(index)
            } else {
                GltfNodeData::Empty
            }
        } else {
            GltfNodeData::Empty
        },
        children: Vec::default(),
    };

    // Load in children
    if let Some(children) = &node.children {
        for child in children {
            out_node
                .children
                .push(parse_node(child.value(), gltf, mapping));
        }
    }

    out_node
}

fn inspect_mesh_group(
    mesh_group_idx: usize,
    gltf: &gltf::json::Root,
    texture_map: &mut HashMap<usize, (usize, TextureUsage)>,
    material_map: &mut HashMap<usize, usize>,
    mesh_map: &mut HashMap<Accessor, Vec<Primitive>>,
    mesh_count: &mut usize,
) {
    let mesh_group = &gltf.meshes[mesh_group_idx];
    for primitive in &mesh_group.primitives {
        // Construct the primitive ID and see if we've got it in the mapping
        let mut prim_id = to_primitive_id(gltf, primitive);

        if prim_id.indices.buffer == usize::MAX {
            println!("WARNING: Primitive in mesh group {mesh_group_idx} has no indices.");
            continue;
        }

        if prim_id.positions.buffer == usize::MAX {
            println!("WARNING: Primtive in mesh group {mesh_group_idx} has no positions.");
            continue;
        }

        // Compare with existing primitives associated with the same index accessor and see if we
        // can merge accessors or have to create a new primitive type.
        let primitives = mesh_map.entry(prim_id.indices).or_default();

        let mut needs_new_primitive = true;
        for primitive in primitives.iter_mut() {
            // If "false" we were able to use this existing primitive, so we can stop searching
            if !prim_id.compare_and_update_primitive(primitive) {
                needs_new_primitive = false;
                break;
            }
        }

        if needs_new_primitive {
            prim_id.mesh_idx = *mesh_count;
            *mesh_count += 1;

            primitives.push(prim_id);
        }

        // Skip if we've seen the material before
        let material = match &primitive.material {
            Some(idx) => {
                let idx = idx.value();
                let new_idx = material_map.len();
                if material_map.contains_key(&idx) {
                    continue;
                }
                material_map.insert(idx, new_idx);
                &gltf.materials[idx]
            }
            None => continue,
        };

        // Check textures
        let pbr = &material.pbr_metallic_roughness;

        if let Some(tex) = &pbr.base_color_texture {
            let idx = tex.index.value();
            let new_idx = texture_map.len();
            let (_, old_usage) = texture_map
                .entry(idx)
                .or_insert((new_idx, TextureUsage::Diffuse));
            if *old_usage != TextureUsage::Diffuse {
                println!(
                    "WARNING: Texture at index `{}` was used as `{:?}` but is now used as `{:?}`.",
                    idx,
                    *old_usage,
                    TextureUsage::Diffuse
                );
            }
        }

        if let Some(tex) = &pbr.metallic_roughness_texture {
            let idx = tex.index.value();
            let new_idx = texture_map.len();
            let (_, old_usage) = texture_map
                .entry(idx)
                .or_insert((new_idx, TextureUsage::MetallicRoughness));
            if *old_usage != TextureUsage::MetallicRoughness {
                println!(
                    "WARNING: Texture at index `{}` was used as `{:?}` but is now used as `{:?}`.",
                    idx,
                    *old_usage,
                    TextureUsage::MetallicRoughness
                );
            }
        }

        if let Some(tex) = &material.normal_texture {
            let idx = tex.index.value();
            let new_idx = texture_map.len();
            let (_, old_usage) = texture_map
                .entry(idx)
                .or_insert((new_idx, TextureUsage::Normal));
            if *old_usage != TextureUsage::Normal {
                println!(
                    "WARNING: Texture at index `{}` was used as `{:?}` but is now used as `{:?}`.",
                    idx,
                    *old_usage,
                    TextureUsage::Normal
                );
            }
        }
    }
}

fn load_gltf_lights(gltf: &gltf::json::Root, mapping: &DataMapping) -> Vec<GltfLight> {
    use rayon::prelude::*;

    let gltf_exts = match gltf.extensions.as_ref() {
        Some(exts) => exts,
        None => return Vec::default(),
    };
    let gltf_lights = match gltf_exts.khr_lights_punctual.as_ref() {
        Some(lights) => &lights.lights,
        None => return Vec::default(),
    };

    (0..mapping.lights.len())
        .into_par_iter()
        .map(|i| {
            let gltf_idx = *mapping.lights.get(&i).unwrap();
            let gltf_light = &gltf_lights[gltf_idx];
            match gltf_light.type_.unwrap() {
                khr_lights_punctual::Type::Directional => GltfLight::Directional {
                    color: Vec3::from_array(gltf_light.color),
                    intensity: gltf_light.intensity,
                },
                khr_lights_punctual::Type::Point => GltfLight::Point {
                    color: Vec3::from_array(gltf_light.color),
                    intensity: gltf_light.intensity,
                    range: gltf_light.range.unwrap_or(f32::INFINITY),
                },
                khr_lights_punctual::Type::Spot => {
                    let args = gltf_light.spot.as_ref().unwrap();
                    GltfLight::Spot {
                        color: Vec3::from_array(gltf_light.color),
                        intensity: gltf_light.intensity,
                        range: gltf_light.range.unwrap_or(f32::INFINITY),
                        inner_angle: args.inner_cone_angle,
                        outer_angle: args.outer_cone_angle,
                    }
                }
            }
        })
        .collect()
}

fn load_gltf_materials(
    gltf: &gltf::json::Root,
    mapping: &DataMapping,
    inv_mapping: &InvDataMapping,
) -> Vec<GltfMaterial> {
    use rayon::prelude::*;

    let gltf_materials = &gltf.materials;
    (0..mapping.materials.len())
        .into_par_iter()
        .map(|i| {
            let gltf_idx = *mapping.materials.get(&i).unwrap();
            let gltf_material = &gltf_materials[gltf_idx];

            GltfMaterial::Pbr {
                base_color: Vec4::from(gltf_material.pbr_metallic_roughness.base_color_factor.0),
                metallic: gltf_material.pbr_metallic_roughness.metallic_factor.0,
                roughness: gltf_material.pbr_metallic_roughness.roughness_factor.0,
                alpha_cutoff: if gltf_material.alpha_mode.unwrap()
                    == gltf::material::AlphaMode::Opaque
                {
                    0.0
                } else {
                    gltf_material.alpha_cutoff.map(|v| v.0).unwrap_or(0.0)
                },
                diffuse_map: gltf_material
                    .pbr_metallic_roughness
                    .base_color_texture
                    .as_ref()
                    .map(|info| inv_mapping.textures.get(&info.index.value()).unwrap().0),
                normal_map: gltf_material
                    .normal_texture
                    .as_ref()
                    .map(|info| inv_mapping.textures.get(&info.index.value()).unwrap().0),
                metallic_roughness_map: gltf_material
                    .pbr_metallic_roughness
                    .metallic_roughness_texture
                    .as_ref()
                    .map(|info| inv_mapping.textures.get(&info.index.value()).unwrap().0),
                blending: match gltf_material.alpha_mode.unwrap() {
                    gltf::material::AlphaMode::Opaque => BlendType::Opaque,
                    gltf::material::AlphaMode::Mask => BlendType::Mask,
                    gltf::material::AlphaMode::Blend => BlendType::Blend,
                },
            }
        })
        .collect()
}

fn load_gltf_textures(
    gltf: &gltf::json::Root,
    mapping: &DataMapping,
    bin: &[u8],
) -> Vec<GltfTexture> {
    use rayon::prelude::*;

    let gltf_textures = &gltf.textures;
    (0..mapping.textures.len())
        .into_par_iter()
        .map(|i| {
            let (gltf_idx, usage) = *mapping.textures.get(&i).unwrap();
            let gltf_texture = &gltf_textures[gltf_idx];
            let gltf_image = &gltf.images[gltf_texture.source.value()];
            let gltf_view = match &gltf_image.buffer_view {
                Some(view) => &gltf.buffer_views[view.value()],
                None => {
                    println!("WARNING: Texture {gltf_idx} is using URI and not a buffer view.");
                    return GltfTexture {
                        data: Vec::default(),
                        src_format: TextureSourceFormat::Png,
                        usage,
                        sampler: GltfSampler::default(),
                        mips: false,
                    };
                }
            };
            let mime_type = match &gltf_image.mime_type {
                Some(mime_type) => mime_type,
                None => {
                    println!("WARNING: Texture {gltf_idx} has an unknown source format.");
                    return GltfTexture {
                        data: Vec::default(),
                        src_format: TextureSourceFormat::Png,
                        usage,
                        sampler: GltfSampler::default(),
                        mips: false,
                    };
                }
            };
            let src_format = match mime_type.0.as_str() {
                "image/jpeg" => TextureSourceFormat::Jpeg,
                "image/png" => TextureSourceFormat::Png,
                _ => {
                    println!("WARNING: Texture {gltf_idx} has an unknown source format.");
                    return GltfTexture {
                        data: Vec::default(),
                        src_format: TextureSourceFormat::Png,
                        usage,
                        sampler: GltfSampler::default(),
                        mips: false,
                    };
                }
            };
            let data = match gltf_view.byte_stride {
                Some(_) => {
                    println!("WARNING: Texture {gltf_idx} is using stride.");
                    return GltfTexture {
                        data: Vec::default(),
                        src_format,
                        usage,
                        sampler: GltfSampler::default(),
                        mips: false,
                    };
                }
                None => {
                    let offset = gltf_view.byte_offset.unwrap_or(0) as usize;
                    let len = gltf_view.byte_length as usize;
                    Vec::from(&bin[offset..(offset + len)])
                }
            };
            let (sampler, mips) = match &gltf_texture.sampler {
                Some(sampler_idx) => {
                    let gltf_sampler = &gltf.samplers[sampler_idx.value()];
                    let max = gltf_to_pal_mag_filter(
                        gltf_sampler
                            .mag_filter
                            .map(|filter| filter.unwrap())
                            .unwrap_or(gltf::texture::MagFilter::Linear),
                    );
                    let (min, mip) = gltf_to_pal_min_filter(
                        gltf_sampler
                            .min_filter
                            .map(|filter| filter.unwrap())
                            .unwrap_or(gltf::texture::MinFilter::Linear),
                    );
                    let wrap_u = gltf_to_pal_wrap_mode(gltf_sampler.wrap_s.unwrap());
                    let wrap_v = gltf_to_pal_wrap_mode(gltf_sampler.wrap_t.unwrap());

                    (
                        GltfSampler {
                            min_filter: min,
                            mag_filter: max,
                            mipmap_filter: mip.unwrap_or(Filter::Linear),
                            address_u: wrap_u,
                            address_v: wrap_v,
                        },
                        mip.is_some(),
                    )
                }
                None => (GltfSampler::default(), false),
            };

            GltfTexture {
                data,
                src_format,
                usage,
                sampler,
                mips,
            }
        })
        .collect()
}

fn load_gltf_meshes(
    gltf: &gltf::json::Root,
    mapping: &InvDataMapping,
    bin: &[u8],
) -> Vec<GltfMesh> {
    // Sort by our index so we can get the correct mapping
    let mut primitives: Vec<_> = mapping
        .meshes
        .clone()
        .into_iter()
        .flat_map(|(_, prim)| prim.into_iter())
        .collect();
    primitives.sort_by_key(|e| e.mesh_idx);

    primitives
        .par_iter()
        .map(|primitive| load_gltf_primitive(gltf, primitive, bin))
        .collect()
}

fn load_gltf_mesh_groups(
    gltf: &gltf::json::Root,
    mapping: &DataMapping,
    inv_mapping: &InvDataMapping,
) -> Vec<GltfMeshGroup> {
    use rayon::prelude::*;

    let gltf_meshes = &gltf.meshes;
    (0..mapping.mesh_groups.len())
        .into_par_iter()
        .map(|i| {
            let gltf_idx = *mapping.mesh_groups.get(&i).unwrap();
            let gltf_mesh = &gltf_meshes[gltf_idx];

            let mut mesh_group = GltfMeshGroup(Vec::with_capacity(gltf_mesh.primitives.len()));
            for primitive in &gltf_mesh.primitives {
                let material = match &primitive.material {
                    Some(material_idx) => {
                        *inv_mapping.materials.get(&material_idx.value()).unwrap()
                    }
                    None => {
                        println!("WARNING: Primitive in mesh group {gltf_idx} has no material.");
                        continue;
                    }
                };

                let prim_id = to_primitive_id(gltf, &primitive);

                // Check each primitive from the associated index buffer and find which one we are
                // a subset of
                let mut mesh_idx = usize::MAX;
                for primitive in inv_mapping.meshes.get(&prim_id.indices).unwrap() {
                    if prim_id.is_subset_of(primitive) {
                        mesh_idx = primitive.mesh_idx;
                        break;
                    }
                }
                assert!(mesh_idx != usize::MAX);

                mesh_group.0.push(GltfMeshInstance {
                    mesh: mesh_idx,
                    material,
                });
            }

            mesh_group
        })
        .collect()
}

fn load_gltf_primitive(gltf: &gltf::json::Root, primitive: &Primitive, bin: &[u8]) -> GltfMesh {
    let positions = match accessor_to_vec::<Vec4>(
        gltf,
        &primitive.positions,
        bin,
        gltf::accessor::DataType::F32,
    ) {
        Some(res) => res,
        None => {
            println!("WARNING: Unable to load primitive.");
            return GltfMesh::default();
        }
    };

    let normals = if let Some(accessor) = primitive.normals {
        match accessor_to_vec::<Vec4>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let tangents = if let Some(accessor) = primitive.tangents {
        match accessor_to_vec::<Vec4>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let colors = if let Some(accessor) = primitive.colors {
        match accessor_to_vec::<Vec4>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let uv0 = if let Some(accessor) = primitive.uv0s {
        match accessor_to_vec::<Vec2>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let uv1 = if let Some(accessor) = primitive.uv1s {
        match accessor_to_vec::<Vec2>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let uv2 = if let Some(accessor) = primitive.uv2s {
        match accessor_to_vec::<Vec2>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    let uv3 = if let Some(accessor) = primitive.uv3s {
        match accessor_to_vec::<Vec2>(gltf, &accessor, bin, gltf::accessor::DataType::F32) {
            Some(res) => res,
            None => {
                println!("WARNING: Unable to load primitive.");
                return GltfMesh::default();
            }
        }
    } else {
        Vec::default()
    };

    // Load in the indices. They are required to be u32 by the GLTF spec
    let indices_accessor = &primitive.indices;

    const U16: u32 = gltf::accessor::DataType::U16 as u32;
    const U32: u32 = gltf::accessor::DataType::U32 as u32;

    let indices = match indices_accessor.component_type {
        U16 => {
            let u16_indices = match accessor_to_vec::<u16>(
                gltf,
                indices_accessor,
                bin,
                gltf::accessor::DataType::U16,
            ) {
                Some(res) => res,
                None => {
                    println!("WARNING: Unable to load primitive.");
                    return GltfMesh::default();
                }
            };
            let mut as_u32 = Vec::with_capacity(u16_indices.len());
            for i in u16_indices {
                as_u32.push(i as u32);
            }

            as_u32
        }
        U32 => {
            match accessor_to_vec::<u32>(gltf, indices_accessor, bin, gltf::accessor::DataType::U32)
            {
                Some(res) => res,
                None => {
                    println!("WARNING: Unable to load primitive.");
                    return GltfMesh::default();
                }
            }
        }
        _ => {
            println!("WARNING: Unsupported index data type.");
            return GltfMesh::default();
        }
    };

    GltfMesh {
        indices,
        positions,
        normals: if normals.is_empty() {
            None
        } else {
            Some(normals)
        },
        tangents: if tangents.is_empty() {
            None
        } else {
            Some(tangents)
        },
        colors: if colors.is_empty() {
            None
        } else {
            Some(colors)
        },
        uv0: if uv0.is_empty() { None } else { Some(uv0) },
        uv1: if uv1.is_empty() { None } else { Some(uv1) },
        uv2: if uv2.is_empty() { None } else { Some(uv2) },
        uv3: if uv3.is_empty() { None } else { Some(uv3) },
    }
}

/// Takes an accessor and turns the data referenced into a buffer of another type.
fn accessor_to_vec<T: Pod + Zeroable + 'static>(
    gltf: &gltf::json::Root,
    accessor: &Accessor,
    raw: &[u8],
    expected_data_type: gltf::accessor::DataType,
) -> Option<Vec<T>> {
    // Don't support non-float data types
    if accessor.component_type != expected_data_type as u32 {
        println!(
            "WARNING: Expected `{:?}` accessor data type but got `{:?}`.",
            expected_data_type, accessor.component_type
        );
        return None;
    }

    let data_size = match expected_data_type {
        gltf::accessor::DataType::I8 => std::mem::size_of::<i8>(),
        gltf::accessor::DataType::U8 => std::mem::size_of::<u8>(),
        gltf::accessor::DataType::I16 => std::mem::size_of::<i16>(),
        gltf::accessor::DataType::U16 => std::mem::size_of::<u16>(),
        gltf::accessor::DataType::U32 => std::mem::size_of::<u32>(),
        gltf::accessor::DataType::F32 => std::mem::size_of::<f32>(),
    };

    // Ensure the buffer is from the binary blob and not a uri
    if gltf.buffers[accessor.buffer].uri.is_some() {
        println!("WARNING: No support for vertex data from URI.");
        return None;
    }

    // Create a raw buffer for the point data
    // NOTE: We have to use unsafe here because bytemuck requires the alignments to be the same.
    // The u8 alignment requirement is less strict than T, so we initialize as T and then convert
    // to u8. Same thing but in reverse happens in the return.
    let mut points = unsafe {
        let mut buf = Vec::<T>::with_capacity(accessor.count as usize);
        let ptr = buf.as_mut_ptr();
        let cap = accessor.count as usize;
        std::mem::forget(buf);
        Vec::<u8>::from_raw_parts(ptr as *mut u8, 0, cap * std::mem::size_of::<T>())
    };
    points.resize(accessor.count as usize * std::mem::size_of::<T>(), 0);

    // Determine strides and sizes for copying data
    let read_size = accessor.component_count as usize * data_size;

    let read_stride = match accessor.byte_stride {
        Some(stride) => stride as usize,
        None => read_size,
    };

    let write_size = std::mem::size_of::<T>();

    // Read size has to be less than or equal to the write size, otherwise we are copying OOB
    if read_size > write_size {
        println!("WARNING: Vertex attribute is bigger than requested type.");
        return None;
    }

    let mut read_offset = accessor.byte_offset as usize;
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
        Some(Vec::<T>::from_raw_parts(
            ptr as *mut T,
            len / std::mem::size_of::<T>(),
            cap / std::mem::size_of::<T>(),
        ))
    }
}

fn to_primitive_id(gltf: &gltf::json::Root, primitive: &gltf::json::mesh::Primitive) -> Primitive {
    let mut prim_id = Primitive {
        mesh_idx: usize::MAX,
        indices: Accessor::default(),
        positions: Accessor::default(),
        normals: None,
        tangents: None,
        colors: None,
        uv0s: None,
        uv1s: None,
        uv2s: None,
        uv3s: None,
    };

    if let Some(id) = &primitive.indices {
        prim_id.indices = to_primitive_accessor(gltf, &gltf.accessors[id.value()]);
    }

    for (attribute, id) in primitive.attributes.iter() {
        match attribute.as_ref().unwrap() {
            gltf::Semantic::Positions => {
                prim_id.positions = to_primitive_accessor(gltf, &gltf.accessors[id.value()]);
            }
            gltf::Semantic::Normals => {
                prim_id.normals = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()]));
            }
            gltf::Semantic::Tangents => {
                prim_id.tangents = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()]));
            }
            gltf::Semantic::Colors(n) => {
                if *n == 0 {
                    prim_id.colors = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()]));
                }
            }
            gltf::Semantic::TexCoords(n) => match n {
                0 => prim_id.uv0s = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()])),
                1 => prim_id.uv1s = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()])),
                2 => prim_id.uv2s = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()])),
                3 => prim_id.uv3s = Some(to_primitive_accessor(gltf, &gltf.accessors[id.value()])),
                _ => {}
            },
            _ => {}
        }
    }

    prim_id
}

fn to_primitive_accessor(gltf: &gltf::json::Root, accessor: &gltf::json::Accessor) -> Accessor {
    let view = match &accessor.buffer_view {
        Some(view) => &gltf.buffer_views[view.value()],
        None => {
            panic!("ERROR: no support for sparse attributes.");
        }
    };

    Accessor {
        buffer: view.buffer.value(),
        component_type: accessor.component_type.unwrap().0 as u32,
        component_count: match accessor.type_.unwrap() {
            gltf::accessor::Dimensions::Scalar => 1,
            gltf::accessor::Dimensions::Vec2 => 2,
            gltf::accessor::Dimensions::Vec3 => 3,
            gltf::accessor::Dimensions::Vec4 => 4,
            _ => 0,
        },
        count: accessor.count,
        byte_offset: accessor.byte_offset + view.byte_offset.unwrap_or(0),
        byte_stride: view.byte_stride,
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
