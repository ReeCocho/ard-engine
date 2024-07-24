use crate::{
    buffer::Buffer,
    command_buffer::Command,
    cube_map::CubeMap,
    descriptor_set::DescriptorSet,
    graphics_pipeline::GraphicsPipeline,
    surface::SurfaceImage,
    texture::Texture,
    types::{
        CubeFace, IndexType, LoadOp, MultiSamples, ResolveMode, Scissor, ShaderStage, StoreOp,
    },
    Backend,
};

/// Describes a render pass.
pub struct RenderPassDescriptor<'a, B: Backend> {
    /// The color attachments used by the render pass.
    pub color_attachments: Vec<ColorAttachment<'a, B>>,
    /// An optional depth stencil attachment used by the render pass.
    pub depth_stencil_attachment: Option<DepthStencilAttachment<'a, B>>,
    /// Color attachments to use for multi-sample resolution.
    pub color_resolve_attachments: Vec<ColorResolveAttachment<'a, B>>,
    /// An optional depth stencil attachment used for multi-sample resolution.
    pub depth_stencil_resolve_attachment: Option<DepthStencilResolveAttachment<'a, B>>,
}

/// Describes a color attachment of a render pass.
pub struct ColorAttachment<'a, B: Backend> {
    /// The destination image of the attachment.
    pub dst: ColorAttachmentDestination<'a, B>,
    /// How the color attachment should be loaded.
    pub load_op: LoadOp,
    /// How the color attachment should be stored.
    pub store_op: StoreOp,
    /// The number of samples to render per fragment.
    pub samples: MultiSamples,
}

/// Describes a color attachment used for multi-sample resolution.
pub struct ColorResolveAttachment<'a, B: Backend> {
    /// The index of the source multi-sampled texture within the `color_attachments` of the render
    /// pass.
    pub src: usize,
    /// The source image of the attachment.
    pub dst: ColorAttachmentDestination<'a, B>,
    /// How the color attachment should be loaded.
    pub load_op: LoadOp,
    /// How the color attachment should be stored.
    pub store_op: StoreOp,
}

/// The destination data of a color attachment.
pub enum ColorAttachmentDestination<'a, B: Backend> {
    SurfaceImage(&'a SurfaceImage<B>),
    Texture {
        texture: &'a Texture<B>,
        array_element: usize,
        mip_level: usize,
    },
    CubeFace {
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        face: CubeFace,
        mip_level: usize,
    },
    CubeMap {
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        mip_level: usize,
    },
}

/// Describes the depth stencil attachment of a render pass.
pub struct DepthStencilAttachment<'a, B: Backend> {
    /// The attachment destination.
    pub dst: DepthStencilAttachmentDestination<'a, B>,
    /// How the depth stencil attachment should be loaded.
    pub load_op: LoadOp,
    /// How the depth stencil attachment should be stored.
    pub store_op: StoreOp,
    /// The number of samples to render per fragment.
    pub samples: MultiSamples,
}

/// Describes the depth stencil attachment used for multi-sample resolution of a render pass.
pub struct DepthStencilResolveAttachment<'a, B: Backend> {
    /// The attachment destination to resolve to.
    pub dst: DepthStencilAttachmentDestination<'a, B>,
    /// How the depth stencil attachment should be loaded.
    pub load_op: LoadOp,
    /// How the depth stencil attachment should be stored.
    pub store_op: StoreOp,
    /// How depth values should be resolved.
    pub depth_resolve_mode: ResolveMode,
    /// How stencil values should be resolved.
    pub stencil_resolve_mode: ResolveMode,
}

/// The destination data of a depth stencil attachment.
pub enum DepthStencilAttachmentDestination<'a, B: Backend> {
    Texture {
        texture: &'a Texture<B>,
        array_element: usize,
        mip_level: usize,
    },
    CubeFace {
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        face: CubeFace,
        mip_level: usize,
    },
    CubeMap {
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        mip_level: usize,
    },
}

pub struct RenderPass<'a, B: Backend> {
    pub(crate) bound_pipeline: bool,
    pub(crate) commands: Vec<Command<'a, B>>,
}

pub struct VertexBind<'a, B: Backend> {
    pub buffer: &'a Buffer<B>,
    pub array_element: usize,
    pub offset: u64,
}

impl<'a, B: Backend> RenderPass<'a, B> {
    /// Binds a graphics pipeline to the pass.
    ///
    /// # Arguments
    /// - `pipeline` - The graphics pipeline to bind.
    #[inline]
    pub fn bind_pipeline(&mut self, pipeline: GraphicsPipeline<B>) {
        self.bound_pipeline = true;
        self.commands.push(Command::BindGraphicsPipeline(pipeline));
    }

    #[inline]
    pub fn push_constants(&mut self, data: &[u8]) {
        self.commands.push(Command::PushConstants {
            data: Vec::from(data),
            stage: ShaderStage::AllGraphics,
        });
    }

    /// Binds one or more descriptor sets to the pass.
    ///
    /// # Arguments
    /// - `first` - An offset added to the set indices. For example, if you wanted to bind only the
    /// second set of your pipeline, you would set `first = 1`.
    /// - `sets` - The sets to bind.
    ///
    /// # Valid Usage
    /// The user *must* ensure that the bound sets do not go out of bounds of the pipeline they are
    /// used in. Backends *should* perform validity checking of set bounds.
    #[inline]
    pub fn bind_sets(&mut self, first: usize, sets: Vec<&'a DescriptorSet<B>>) {
        self.commands.push(Command::BindDescriptorSets {
            sets,
            first,
            stage: ShaderStage::AllGraphics,
        });
    }

    /// Allows you to bind descriptor sets without performing validation and synchronization
    /// checks.
    ///
    /// # Arguments
    /// Arguments are the same as the `RenderPass::bind_sets`.
    ///
    /// # Safety
    /// For the use of this command to be safe, you must guarantee:
    /// 1. All bound resources must have been verified/synchornized by a previous command within
    ///    the same command buffer.
    /// 2. The previous usage of each resource must have been read only.
    /// 3. The next usage of each resource must be read only.
    /// #[inline]
    pub unsafe fn bind_sets_unchecked(&mut self, first: usize, sets: Vec<&'a DescriptorSet<B>>) {
        self.commands.push(Command::BindDescriptorSetsUnchecked {
            sets,
            first,
            stage: ShaderStage::AllGraphics,
        });
    }

    /// Binds vertex buffers to the pass.
    ///
    /// # Arguments
    /// - `first` - An offset added to the bind indices. For example, if you wanted to bind only
    /// the second binding, you would set `first = 1`.
    /// - `binds` - The vertex buffers to bind.
    ///
    /// # Panics
    /// - If `binds.is_empty()`.
    ///
    /// # Valid Usage
    /// The user *must* ensure that the bound buffers do not go out of bounds of the pipeline they
    /// are used in. Backends *should* perform validity checking of the set bounds.
    #[inline]
    pub fn bind_vertex_buffers(&mut self, first: usize, binds: Vec<VertexBind<'a, B>>) {
        self.commands
            .push(Command::BindVertexBuffers { first, binds });
    }

    /// Binds an index buffer to the pass.
    ///
    /// # Arguments
    /// - `buffer` - The index buffer to bind.
    /// - `array_element` - The array element of the index buffer to bind.
    /// - `offset` - The offset within the array element of the index buffer to bind.
    /// - `ty` - The type of indices contained within the buffer.
    #[inline]
    pub fn bind_index_buffer(
        &mut self,
        buffer: &'a Buffer<B>,
        array_element: usize,
        offset: u64,
        ty: IndexType,
    ) {
        self.commands.push(Command::BindIndexBuffer {
            buffer,
            array_element,
            offset,
            ty,
        });
    }

    /// Sets the scissor area to render.
    ///
    /// # Arguments
    /// - `idx` - The index of the attachment to apply the scissor to.
    /// - `scissor` - The scissor value.
    #[inline]
    pub fn set_scissor(&mut self, attachment: usize, scissor: Scissor) {
        self.commands.push(Command::Scissor {
            attachment,
            scissor,
        });
    }

    /// Draws an unindexed sequence of triangles.
    ///
    /// # Arguments
    /// - `vertex_count` - The number of vertices to use.
    /// - `instance_count` - The number of instances to draw.
    /// - `first_vertex` - The offset in vertices within the vertex buffers.
    /// - `first_instance` - The offset in instances to draw.
    ///
    /// # Panics
    /// - If `vertex_count = 0`.
    /// - If `instance_count = 0`.
    #[inline]
    pub fn draw(
        &mut self,
        vertex_count: usize,
        instance_count: usize,
        first_vertex: usize,
        first_instance: usize,
    ) {
        assert_ne!(vertex_count, 0, "vertex count cannot be 0");
        assert_ne!(instance_count, 0, "instance count cannot be 0");
        self.commands.push(Command::Draw {
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        });
    }

    /// Draw an indexed sequence of triangles.
    ///
    /// # Arguments
    /// - `index_count` - The number of indices to use.
    /// - `instance_count` - The number of instances to draw.
    /// - `first_index` - The offset in indices within the index buffer.
    /// - `vertex_offset` - The offset in vertices within the vertex buffers.
    /// - `first_instance` - The offset in instances to draw.
    ///
    /// # Panics
    /// - If `index_count = 0`.
    /// - If `instance_count = 0`.
    #[inline]
    pub fn draw_indexed(
        &mut self,
        index_count: usize,
        instance_count: usize,
        first_index: usize,
        vertex_offset: isize,
        first_instance: usize,
    ) {
        assert_ne!(index_count, 0, "index count cannot be 0");
        assert_ne!(instance_count, 0, "instance count cannot be 0");
        self.commands.push(Command::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        });
    }

    /// Draw an indexed sequence of triangles with draw commands contained within an indirect
    /// buffer.
    ///
    /// # Arguments
    /// - `buffer` - The indirect buffer to read commands from.
    /// - `array_element` - The array element of the indirect buffer to read from.
    /// - `offset` - The offset in bytes within the array element to read from.
    /// - `draw_count` - The number of draw commands to read.
    /// - `stride` - The stride in bytes for each draw command.
    ///
    /// # Panics
    /// - If `stride == 0`.
    #[inline]
    pub fn draw_indexed_indirect(
        &mut self,
        buffer: &'a Buffer<B>,
        array_element: usize,
        offset: u64,
        draw_count: usize,
        stride: u64,
    ) {
        assert_ne!(stride, 0, "stride cannot be 0");
        self.commands.push(Command::DrawIndexedIndirect {
            buffer,
            array_element,
            offset,
            draw_count,
            stride,
        });
    }

    /// Draw an indexed sequence of triangles with draw commands contained within an indirect
    /// buffer. An unsigned 32-bit draw count is sourced from an alternative buffer.
    ///
    /// # Arguments
    /// - `draw_buffer` - The indirect buffer to read commands from.
    /// - `draw_array_element` - The array element of the indirect buffer to read from.
    /// - `draw_offset` - The offset in bytes within the indirect buffer array element to read
    /// from.
    /// - `count_buffer` - The buffer to read draw counts from.
    /// - `count_array_element` - The array element of the draw count buffer to read from.
    /// - `count_offset` - The offset in bytes within the draw count buffer array element to read
    /// from.
    /// - `max_draw_count` - The maximum number of draw commands to read. The actual draw count is
    /// the minimum of `max_draw_count` and the value read from the `count_buffer`.
    /// - `draw_stride` - The stride in bytes for each draw command.
    ///
    /// # Panics
    /// - If `draw_stride == 0`.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_indirect_count(
        &mut self,
        draw_buffer: &'a Buffer<B>,
        draw_array_element: usize,
        draw_offset: u64,
        count_buffer: &'a Buffer<B>,
        count_array_element: usize,
        count_offset: u64,
        max_draw_count: usize,
        draw_stride: u64,
    ) {
        assert_ne!(draw_stride, 0, "draw stride cannot be 0");
        self.commands.push(Command::DrawIndexedIndirectCount {
            draw_buffer,
            draw_array_element,
            draw_offset,
            draw_stride,
            count_buffer,
            count_array_element,
            count_offset,
            max_draw_count,
        });
    }

    #[inline]
    pub fn draw_mesh_tasks(&mut self, x: u32, y: u32, z: u32) {
        debug_assert_ne!(x, 0);
        debug_assert_ne!(y, 0);
        debug_assert_ne!(z, 0);
        self.commands.push(Command::DrawMeshTasks(x, y, z));
    }
}
