use std::sync::Arc;

use crate::{
    context::Context, descriptor_set::DescriptorSetLayout, shader::Shader, types::*, Backend,
};
use thiserror::Error;

/// The shader stages used by a graphics pipeline.
#[derive(Clone)]
pub struct ShaderStages<B: Backend> {
    pub vertex: Shader<B>,
    pub fragment: Option<Shader<B>>,
}

/// Describes a vertex attribute.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VertexInputAttribute {
    /// The binding this vertex attribute is associated with.
    pub binding: u32,
    /// The location within the binding this attribute is bound to.
    pub location: u32,
    /// The data format of the attribute.
    pub format: VertexFormat,
    /// The offset in bytes within the binding the attribute is located at.
    pub offset: u32,
}

/// Describes a binding of multiple vertex attributes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VertexInputBinding {
    /// The id of the binding.
    pub binding: u32,
    /// The stride in bytes of each element of the binding.
    pub stride: u32,
    /// The rate at which the attributes of this binding are provided.
    pub input_rate: VertexInputRate,
}

/// Describes the vertex inputs of a graphics pipeline.
#[derive(Clone)]
pub struct VertexInputState {
    /// The attributes of each binding.
    pub attributes: Vec<VertexInputAttribute>,
    /// The bindings to the pipeline. Each binding represents a different buffer.
    pub bindings: Vec<VertexInputBinding>,
    /// How to connect the vertices to form primitives.
    pub topology: PrimitiveTopology,
}

/// Describes how rasterization should be performed for the pipeline.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RasterizationState {
    /// The kinds of primitives to form.
    pub polygon_mode: PolygonMode,
    /// Culling rule for primitives.
    pub cull_mode: CullMode,
    /// Which direction represents the front face of a primitive.
    pub front_face: FrontFace,
}

/// Describes depth testing rules for a graphics pipeline.
#[derive(Clone, Copy)]
pub struct DepthStencilState {
    /// Should depth values be clamped to the provided `min_depth` and `max_depth`.
    pub depth_clamp: bool,
    /// Should depth testing be enabled.
    pub depth_test: bool,
    /// Should we write to the depth buffer.
    pub depth_write: bool,
    /// What comparison operation should be used to pass depth values.
    pub depth_compare: CompareOp,
    /// Minimum value for depth values.
    ///
    /// Ignored if `depth_clamp = false`.
    pub min_depth: f32,
    /// Maximum value for depth values.
    ///
    /// Ignored if `depth_clamp = false`.
    pub max_depth: f32,
}

/// Describes blending operations for color attachments of a graphics pipeline.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ColorBlendAttachment {
    /// What components should be written to the color attachment.
    pub write_mask: ColorComponents,
    /// Should blending be performed.
    pub blend: bool,
    /// What blending operations should be used for color values.
    pub color_blend_op: BlendOp,
    pub src_color_blend_factor: BlendFactor,
    pub dst_color_blend_factor: BlendFactor,
    pub alpha_blend_op: BlendOp,
    pub src_alpha_blend_factor: BlendFactor,
    pub dst_alpha_blend_factor: BlendFactor,
}

/// Blending for color attachments.
#[derive(Default, Clone)]
pub struct ColorBlendState {
    /// Each color attachment to blend and how.
    pub attachments: Vec<ColorBlendAttachment>,
}

#[derive(Clone)]
pub struct GraphicsPipelineCreateInfo<B: Backend> {
    pub stages: ShaderStages<B>,
    pub layouts: Vec<DescriptorSetLayout<B>>,
    pub vertex_input: VertexInputState,
    pub rasterization: RasterizationState,
    pub depth_stencil: Option<DepthStencilState>,
    pub color_blend: Option<ColorBlendState>,
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

pub struct GraphicsPipeline<B: Backend>(pub(crate) Arc<GraphicsPipelineInner<B>>);

pub(crate) struct GraphicsPipelineInner<B: Backend> {
    ctx: Context<B>,
    pub(crate) layouts: Vec<DescriptorSetLayout<B>>,
    pub(crate) id: B::GraphicsPipeline,
}

#[derive(Debug, Error)]
pub enum GraphicsPipelineCreateError {
    #[error("no vertex attributes or bindings were provided")]
    NoAttributesOrBindings,
    #[error("no depth/stencil or color attachments provided")]
    NoAttachments,
    #[error("an error occured: {0}")]
    Other(String),
}

impl<B: Backend> GraphicsPipeline<B> {
    /// Create a new graphics pipeline.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `create_info` - Describes the graphics pipeline to create.
    pub fn new(
        ctx: Context<B>,
        create_info: GraphicsPipelineCreateInfo<B>,
    ) -> Result<Self, GraphicsPipelineCreateError> {
        let layouts = create_info.layouts.clone();
        let id = unsafe { ctx.0.create_graphics_pipeline(create_info)? };
        Ok(Self(Arc::new(GraphicsPipelineInner { ctx, id, layouts })))
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::GraphicsPipeline {
        &self.0.id
    }

    #[inline(always)]
    pub fn layouts(&self) -> &[DescriptorSetLayout<B>] {
        &self.0.layouts
    }
}

impl<B: Backend> Clone for GraphicsPipeline<B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<B: Backend> Drop for GraphicsPipelineInner<B> {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_graphics_pipeline(&mut self.id);
        }
    }
}

impl Default for VertexInputState {
    #[inline(always)]
    fn default() -> Self {
        Self {
            attributes: Vec::default(),
            bindings: Vec::default(),
            topology: PrimitiveTopology::TriangleList,
        }
    }
}

impl Default for RasterizationState {
    #[inline(always)]
    fn default() -> Self {
        Self {
            polygon_mode: PolygonMode::Fill,
            cull_mode: CullMode::Back,
            front_face: FrontFace::CounterClockwise,
        }
    }
}

impl Default for DepthStencilState {
    #[inline(always)]
    fn default() -> Self {
        Self {
            depth_clamp: false,
            depth_test: false,
            depth_write: false,
            depth_compare: CompareOp::Always,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }
}

impl Default for ColorBlendAttachment {
    #[inline(always)]
    fn default() -> Self {
        Self {
            write_mask: ColorComponents::empty(),
            blend: false,
            color_blend_op: BlendOp::Add,
            src_color_blend_factor: BlendFactor::One,
            dst_color_blend_factor: BlendFactor::Zero,
            alpha_blend_op: BlendOp::Add,
            src_alpha_blend_factor: BlendFactor::One,
            dst_alpha_blend_factor: BlendFactor::Zero,
        }
    }
}
