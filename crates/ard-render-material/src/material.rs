use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

use ard_formats::vertex::VertexLayout;
use ard_pal::prelude::{
    ColorBlendState, Context, DepthStencilState, GraphicsPipeline, GraphicsPipelineCreateError,
    GraphicsPipelineCreateInfo, MeshShadingShader, RasterizationState, ShaderStage, ShaderStages,
    VertexInputState,
};
use ard_render_base::{
    resource::{ResourceAllocator, ResourceHandle, ResourceId},
    RenderingMode,
};
use ard_render_si::types::*;
use rustc_hash::FxHashMap;
use thiserror::Error;

use crate::{
    binding_table::BindingTableOffset,
    factory::{MaterialFactory, PassId},
    shader::{Shader, ShaderResource},
};

pub struct MaterialCreateInfo {
    /// Variants of this material.
    pub variants: Vec<MaterialVariantDescriptor>,
    pub rt_variants: Vec<RtVariantDescriptor>,
    /// The size of the properties data structure used for this material type.
    pub data_size: u32,
    /// The number of textures this material supports.
    pub texture_slots: u32,
}

pub struct MaterialVariantDescriptor {
    /// The pass this variant supports
    pub pass_id: PassId,
    /// The minimum required vertex attributes for this variant.
    pub vertex_layout: VertexLayout,
    /// The task shader for this variant.
    pub task_shader: Shader,
    /// The mesh shader for this variant.
    pub mesh_shader: Shader,
    /// The fragment shader for this variant.
    pub fragment_shader: Option<Shader>,
    /// How this variant rasterizes triangles.
    pub rasterization: RasterizationState,
    /// How this variant reads/writes the depth buffer.
    pub depth_stencil: Option<DepthStencilState>,
    /// How this variant reads/writes color buffers.
    pub color_blend: ColorBlendState,
    /// Helpful debugging name for this variant.
    pub debug_name: Option<String>,
}

pub struct RtMaterialVariants {
    /// Maps from the vertex layout and rendering mode to the offset in the SBT.
    offsets: BTreeMap<(VertexLayout, RenderingMode), u32>,
    /// Maps from a given pass to another map from the SBT offset to the variant.
    pass_to_variant: BTreeMap<PassId, BTreeMap<u32, RtMaterialVariant>>,
}

pub struct RtVariantDescriptor {
    pub pass_id: PassId,
    pub vertex_layout: VertexLayout,
    pub rendering_mode: RenderingMode,
    pub shader: Shader,
    pub stage: ShaderStage,
}

#[derive(Debug, Error)]
pub enum MaterialCreateError {
    #[error("material must have at least one variant")]
    NoVariants,
    #[error("provided shaders have mismatching texture slots")]
    MismatchingTextureSlots,
    #[error("provided shaders have mismatching data size")]
    MismatchingDataSize,
    #[error("variant `{0}` requires pass `{1:?}` that does not exist")]
    PassDoesNotExist(usize, PassId),
    #[error("variant `{0}` and `{1}` are indentical")]
    DuplicateVariant(usize, usize),
    #[error("variant `{0}` has incompatible depth/stencil state required by pass `{1:?}`")]
    IncompatibleDepthStencil(usize, PassId),
    #[error("variant `{0}` has incompatible color blend states required by pass `{1:?}`")]
    IncompatibleColorAttachments(usize, PassId),
    #[error("variant `{0}` does not have fragment shader required by pass `{1:?}`")]
    MissingFragmentShader(usize, PassId),
    #[error("gpu error: {0}")]
    GpuError(GraphicsPipelineCreateError),
}

/// A set of shader variants for different passes and a particular vertex layout.
///
/// Defines the interface for setting material properties (textures and material data).
#[derive(Clone)]
pub struct Material {
    data_size: u32,
    texture_slots: u32,
    handle: ResourceHandle,
    rt_variants: Arc<Mutex<RtMaterialVariants>>,
}

pub struct MaterialVariant {
    /// Unique identifier for this variant.
    pub id: u32,
    /// The actual pipeline for this variant.
    pub pipeline: GraphicsPipeline,
    /// Pass this variant supports.
    pub pass_id: PassId,
    /// Required vertex attributes for this variant.
    pub vertex_layout: VertexLayout,
}

pub struct RtMaterialVariant {
    pub shader: Shader,
    pub stage: ShaderStage,
}

/// Requests a variant from a material.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialVariantRequest {
    /// The pass we want.
    pub pass_id: PassId,
    /// The vertex attributes we require.
    pub vertex_layout: VertexLayout,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RtMaterialVariantRequest {
    /// The pass we want.
    pub pass_id: PassId,
    /// The vertex attributes we require.
    pub vertex_layout: VertexLayout,
}

pub struct MaterialResource {
    pub data_size: u32,
    pub texture_slots: u32,
    /// Variants the material supports.
    pub variants: Vec<MaterialVariant>,
    pub rt_variants: Arc<Mutex<RtMaterialVariants>>,
    /// Lookup table for pipelines based on their variant.
    pub variant_lookup: Mutex<FxHashMap<MaterialVariantRequest, usize>>,
}

impl Material {
    pub fn new(
        handle: ResourceHandle,
        data_size: u32,
        texture_slots: u32,
        rt_variants: Arc<Mutex<RtMaterialVariants>>,
    ) -> Material {
        Material {
            handle,
            data_size,
            texture_slots,
            rt_variants,
        }
    }

    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.handle.id()
    }

    #[inline(always)]
    pub fn data_size(&self) -> u32 {
        self.data_size
    }

    #[inline(always)]
    pub fn texture_slots(&self) -> u32 {
        self.texture_slots
    }

    #[inline(always)]
    pub fn rt_variants(&self) -> MutexGuard<RtMaterialVariants> {
        self.rt_variants.lock().unwrap()
    }
}

impl MaterialResource {
    pub fn new<const FRAMES_IN_FLIGHT: usize>(
        ctx: &Context,
        factory: &MaterialFactory<FRAMES_IN_FLIGHT>,
        shaders: &ResourceAllocator<ShaderResource, FRAMES_IN_FLIGHT>,
        bt_offsets: &mut BindingTableOffset,
        create_info: MaterialCreateInfo,
    ) -> Result<Self, MaterialCreateError> {
        // Must have at least one variant
        if create_info.variants.is_empty() {
            return Err(MaterialCreateError::NoVariants);
        }

        // Must have identical texture slots and data size
        // NOTE: Safe to unwrap since we varified there is at least one variant
        let (texture_slots, data_size) = {
            let variant = create_info.variants.first().unwrap();
            (
                variant.mesh_shader.texture_slots(),
                variant.mesh_shader.data_size(),
            )
        };

        // Verify the variants of the material make sense
        for (i, variant) in create_info.variants.iter().enumerate() {
            // Pass must exist
            let pass = match factory.get_pass(variant.pass_id) {
                Some(pass) => pass,
                None => return Err(MaterialCreateError::PassDoesNotExist(i, variant.pass_id)),
            };

            // Must have matching texture slots and data size
            if texture_slots != variant.mesh_shader.texture_slots() {
                return Err(MaterialCreateError::MismatchingTextureSlots);
            }

            if data_size != variant.mesh_shader.data_size() {
                return Err(MaterialCreateError::MismatchingDataSize);
            }

            if let Some(fs) = &variant.fragment_shader {
                if texture_slots != fs.texture_slots() {
                    return Err(MaterialCreateError::MismatchingTextureSlots);
                }

                if data_size != fs.data_size() {
                    return Err(MaterialCreateError::MismatchingDataSize);
                }
            }

            // Must have matching depth/stencil
            if pass.has_depth_stencil_attachment != variant.depth_stencil.is_some() {
                return Err(MaterialCreateError::IncompatibleDepthStencil(
                    i,
                    variant.pass_id,
                ));
            }

            // Must have matching color attachments
            if pass.color_attachment_count != variant.color_blend.attachments.len() {
                return Err(MaterialCreateError::IncompatibleColorAttachments(
                    i,
                    variant.pass_id,
                ));
            }

            // Must have fragment shader if pass has color attachments
            if pass.color_attachment_count > 0 && variant.fragment_shader.is_none() {
                return Err(MaterialCreateError::MissingFragmentShader(
                    i,
                    variant.pass_id,
                ));
            }

            // Must not be a duplicate
            let mut duplicate_iter = create_info
                .variants
                .iter()
                .enumerate()
                // Filter out non-duplicates
                .filter(|(j, other_variant)| {
                    i != *j
                        && variant.vertex_layout == other_variant.vertex_layout
                        && variant.pass_id == other_variant.pass_id
                });

            if let Some((j, _)) = duplicate_iter.next() {
                return Err(MaterialCreateError::DuplicateVariant(i, j));
            }
        }

        // TODO: RT variant verification.
        let rt_variants = RtMaterialVariants::new(create_info.rt_variants, bt_offsets);

        // Create each variant
        let mut resource = MaterialResource {
            variants: Vec::with_capacity(create_info.variants.len()),
            rt_variants: Arc::new(Mutex::new(rt_variants)),
            variant_lookup: Mutex::new(FxHashMap::default()),
            data_size: create_info.data_size,
            texture_slots: create_info.texture_slots,
        };

        for variant_desc in create_info.variants {
            // Safe to unwrap since we previously verified that all passes exist
            let pass = factory.get_pass(variant_desc.pass_id).unwrap();

            // Safe to unwrap shaders since they must exist if we have handles to them.
            let task = shaders.get(variant_desc.task_shader.id()).unwrap();
            let mesh = shaders.get(variant_desc.mesh_shader.id()).unwrap();
            let stages = ShaderStages::MeshShading {
                task: Some(MeshShadingShader {
                    shader: task.shader.clone(),
                    work_group_size: task.work_group_size,
                }),
                mesh: MeshShadingShader {
                    shader: mesh.shader.clone(),
                    work_group_size: mesh.work_group_size,
                },
                fragment: variant_desc
                    .fragment_shader
                    .map(|fs| shaders.get(fs.id()).unwrap().shader.clone()),
            };

            resource.variants.push(MaterialVariant {
                id: resource.variants.len() as u32,
                pipeline: GraphicsPipeline::new(
                    ctx.clone(),
                    GraphicsPipelineCreateInfo {
                        stages,
                        layouts: pass.layouts.clone(),
                        vertex_input: VertexInputState::default(),
                        rasterization: variant_desc.rasterization,
                        depth_stencil: variant_desc.depth_stencil,
                        color_blend: variant_desc.color_blend,
                        push_constants_size: Some(
                            std::mem::size_of::<GpuDrawPushConstants>() as u32
                        ),
                        debug_name: variant_desc.debug_name,
                    },
                )?,
                pass_id: variant_desc.pass_id,
                vertex_layout: variant_desc.vertex_layout,
            });
        }

        Ok(resource)
    }

    /// Gets a particular variant of this material based on the provided request.
    ///
    /// Returns `None` if there are no matching variants.
    pub fn get_variant(&self, variant_req: MaterialVariantRequest) -> Option<&MaterialVariant> {
        let mut lookup = self.variant_lookup.lock().unwrap();

        // See if the variant already exists
        if let Some(variant) = lookup.get(&variant_req) {
            return Some(&self.variants[*variant]);
        }

        // If it doesn't, we might be able to create it...

        // Look over all the pipelines and find the one that has the required pass and has the most
        // specialized vertex attributes.
        let mut new_variant = None;
        let mut supported_attributes = 0;

        self.variants
            .iter()
            .enumerate()
            .filter(|(_, variant)| variant.pass_id == variant_req.pass_id)
            .for_each(|(idx, variant)| {
                // Only compatible if the variant requires a subset of the requested vertex
                // attributes
                if !variant.vertex_layout.subset_of(variant_req.vertex_layout) {
                    return;
                }

                // Replace the selected variant if this variants vertex attributes are more
                // specialized
                let attributes = variant.vertex_layout.into_iter().count();
                if attributes > supported_attributes {
                    supported_attributes = attributes;
                    new_variant = Some(idx);
                }
            });

        // If we didn't find a new variant, return early
        let new_variant = match new_variant {
            Some(idx) => idx,
            None => return None,
        };

        // Otherwise, register this new variant in the lookup table
        lookup.insert(variant_req, new_variant);

        Some(&self.variants[new_variant])
    }

    /// Gets a particular variant of this material by ID.
    ///
    /// Returns `None` if the variant ID was invalid.
    #[inline(always)]
    pub fn get_variant_by_id(&self, id: u32) -> Option<&MaterialVariant> {
        self.variants.get(id as usize)
    }
}

impl RtMaterialVariants {
    pub fn new(variants: Vec<RtVariantDescriptor>, bt_offsets: &mut BindingTableOffset) -> Self {
        // Acquire a unique offset for each vertex layout/rendering mode pair
        let mut offsets = BTreeMap::default();
        variants.iter().for_each(|variant| {
            let pair = (variant.vertex_layout, variant.rendering_mode);
            offsets.entry(pair).or_insert_with(|| bt_offsets.register());
        });

        // Construct variants
        let mut pass_to_variant = BTreeMap::<PassId, BTreeMap<u32, RtMaterialVariant>>::default();
        for variant in variants {
            let entry = pass_to_variant.entry(variant.pass_id).or_default();

            // Get the offset for this variant
            // NOTE: Safe to unwrap since we just constructed an offset for each unique pair
            let pair = (variant.vertex_layout, variant.rendering_mode);
            let offset = *offsets.get(&pair).unwrap();

            // Construct the variant
            entry.insert(
                offset,
                RtMaterialVariant {
                    shader: variant.shader,
                    stage: variant.stage,
                },
            );
        }

        Self {
            offsets,
            pass_to_variant,
        }
    }

    // Get the variants for a given pass.
    #[inline(always)]
    pub fn variants_of(&self, pass_id: PassId) -> &BTreeMap<u32, RtMaterialVariant> {
        self.pass_to_variant.get(&pass_id).unwrap()
    }

    // Get the offset for the given vertex layout and rendering mode.
    #[inline(always)]
    pub fn offset_of(
        &mut self,
        vertex_layout: VertexLayout,
        rendering_mode: RenderingMode,
    ) -> Option<u32> {
        // Try to lookup the pair in the map
        let pair = (vertex_layout, rendering_mode);
        if let Some(offset) = self.offsets.get(&pair) {
            return Some(*offset);
        }

        // If we didn't find it, it might be possible to find one that supports a subset of
        // our attributes.
        let mut new_variant = None;
        let mut supported_attributes = 0;

        self.offsets
            .iter()
            .for_each(|((new_vertex_layout, new_mode), offset)| {
                // Rendering modes must match
                if *new_mode != rendering_mode {
                    return;
                }

                // Only compatible if the variant requires a subset of the requested vertex
                // attributes
                if !new_vertex_layout.subset_of(vertex_layout) {
                    return;
                }

                // Replace the selected variant if this variants vertex attributes are more
                // specialized
                let attributes = new_vertex_layout.into_iter().count();
                if attributes > supported_attributes {
                    supported_attributes = attributes;
                    new_variant = Some(*offset);
                }
            });

        // If we still haven't found it, error out
        let offset = new_variant?;

        // Register new offset for next time
        self.offsets.insert(pair, offset);

        Some(offset)
    }
}

impl From<GraphicsPipelineCreateError> for MaterialCreateError {
    fn from(value: GraphicsPipelineCreateError) -> Self {
        MaterialCreateError::GpuError(value)
    }
}
