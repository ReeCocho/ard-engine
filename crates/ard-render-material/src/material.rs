use std::sync::Mutex;

use ard_formats::mesh::VertexLayout;
use ard_pal::prelude::{
    ColorBlendState, Context, DepthStencilState, GraphicsPipeline, GraphicsPipelineCreateError,
    GraphicsPipelineCreateInfo, RasterizationState, ShaderStages,
};
use ard_render_base::resource::{ResourceAllocator, ResourceHandle, ResourceId};
use fxhash::FxHashMap;
use thiserror::Error;

use crate::{
    factory::{MaterialFactory, PassId},
    shader::{Shader, ShaderResource},
};

pub struct MaterialCreateInfo {
    /// Variants of this material.
    pub variants: Vec<MaterialVariantDescriptor>,
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
    /// The vertex shader for this variant.
    pub vertex_shader: Shader,
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

/// Requests a variant from a material.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialVariantRequest {
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
    /// Lookup table for pipelines based on their variant.
    pub variant_lookup: Mutex<FxHashMap<MaterialVariantRequest, usize>>,
}

impl Material {
    pub fn new(handle: ResourceHandle, data_size: u32, texture_slots: u32) -> Material {
        Material {
            handle,
            data_size,
            texture_slots,
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
}

impl MaterialResource {
    pub fn new<const FRAMES_IN_FLIGHT: usize>(
        ctx: &Context,
        factory: &MaterialFactory<FRAMES_IN_FLIGHT>,
        shaders: &ResourceAllocator<ShaderResource, FRAMES_IN_FLIGHT>,
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
                variant.vertex_shader.texture_slots(),
                variant.vertex_shader.data_size(),
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
            if texture_slots != variant.vertex_shader.texture_slots() {
                return Err(MaterialCreateError::MismatchingTextureSlots);
            }

            if data_size != variant.vertex_shader.data_size() {
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

        // Create each variant
        let mut resource = MaterialResource {
            variants: Vec::with_capacity(create_info.variants.len()),
            variant_lookup: Mutex::new(FxHashMap::default()),
            data_size: create_info.data_size,
            texture_slots: create_info.texture_slots,
        };

        for variant_desc in create_info.variants {
            // Safe to unwrap since we previously verified that all passes exist
            let pass = factory.get_pass(variant_desc.pass_id).unwrap();

            // Safe to unwrap shaders since they must exist if we have handles to them.
            let stages = ShaderStages {
                vertex: shaders
                    .get(variant_desc.vertex_shader.id())
                    .unwrap()
                    .shader
                    .clone(),
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
                        vertex_input: variant_desc.vertex_layout.vertex_input_state(),
                        rasterization: variant_desc.rasterization,
                        depth_stencil: variant_desc.depth_stencil,
                        color_blend: variant_desc.color_blend,
                        push_constants_size: None,
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

impl From<GraphicsPipelineCreateError> for MaterialCreateError {
    fn from(value: GraphicsPipelineCreateError) -> Self {
        MaterialCreateError::GpuError(value)
    }
}
