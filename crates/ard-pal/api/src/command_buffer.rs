use crate::{
    buffer::Buffer,
    compute_pass::ComputePass,
    compute_pipeline::ComputePipeline,
    cube_map::CubeMap,
    descriptor_set::DescriptorSet,
    graphics_pipeline::GraphicsPipeline,
    render_pass::{RenderPass, RenderPassDescriptor, VertexBind},
    surface::SurfaceImage,
    texture::{Blit, Texture},
    types::{
        BufferUsage, CubeFace, Filter, IndexType, QueueType, Scissor, ShaderStage, SharingMode,
        TextureUsage,
    },
    Backend,
};

pub enum BlitSource<'a, B: Backend> {
    Texture(&'a Texture<B>),
    CubeMap {
        cube_map: &'a CubeMap<B>,
        face: CubeFace,
    },
}

pub enum BlitDestination<'a, B: Backend> {
    Texture(&'a Texture<B>),
    CubeMap {
        cube_map: &'a CubeMap<B>,
        face: CubeFace,
    },
    SurfaceImage(&'a SurfaceImage<B>),
}

pub struct CopyBufferToBuffer<'a, B: Backend> {
    /// The source buffer to read from.
    pub src: &'a Buffer<B>,
    /// The source array element to read from.
    pub src_array_element: usize,
    /// The offset within the array element of the source buffer to read from.
    pub src_offset: u64,
    /// The destination buffer to write to.
    pub dst: &'a Buffer<B>,
    /// The destination array element to write to.
    pub dst_array_element: usize,
    /// The offset within the array element of the destination buffer to write to.
    pub dst_offset: u64,
    /// The number of bytes to copy.
    pub len: u64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferTextureCopy {
    /// Offset from the start of the buffer array element to begin read/write.
    pub buffer_offset: u64,
    /// In combination with `buffer_image_height`, this defines (in texels) as subregion of a
    /// larger texture in buffer memory, controling addressing calcualtions. If either value is
    /// zero, the buffer memory is considered tightly packed.
    pub buffer_row_length: u32,
    /// See `buffer_row_length`.
    pub buffer_image_height: u32,
    /// The array element of the buffer to read/write.
    pub buffer_array_element: usize,
    /// The width, height, and depth offsets within the texture to read/write.
    pub texture_offset: (u32, u32, u32),
    /// The width, height, and depth sizes within the texture to read/write.
    pub texture_extent: (u32, u32, u32),
    /// The mip level of the texture to read/write.
    pub texture_mip_level: usize,
    /// The array element of the texture to read/write.
    pub texture_array_element: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferCubeMapCopy {
    /// Offset from the start of the buffer array element to begin read/write.
    pub buffer_offset: u64,
    /// The array element of the buffer to read/write.
    pub buffer_array_element: usize,
    /// The mip level of the cube map to read/write.
    pub cube_map_mip_level: usize,
    /// The array element of the texture to read/write.
    pub cube_map_array_element: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureResolve {
    /// Source texture array element to resolve.
    pub src_array_element: usize,
    /// Source texture mip level to resolve.
    pub src_mip: usize,
    /// Destination texture array element to resolve to.
    pub dst_array_element: usize,
    /// Destination texture mip level to resolve to.
    pub dst_mip: usize,
    /// Offset in pixels within the source texture to begin resolving.
    pub src_offset: (i32, i32, i32),
    /// Offset in pixels within the destination texture to resolve to.
    pub dst_offset: (i32, i32, i32),
    /// Pixel extents to resolve.
    pub extent: (u32, u32, u32),
}

pub enum Command<'a, B: Backend> {
    BeginRenderPass(RenderPassDescriptor<'a, B>, Option<&'a str>),
    EndRenderPass(Option<&'a str>),
    BeginComputePass(ComputePipeline<B>, Option<&'a str>),
    EndComputePass(u32, u32, u32, Option<&'a str>),
    BindGraphicsPipeline(GraphicsPipeline<B>),
    PushConstants {
        stage: ShaderStage,
        data: Vec<u8>,
    },
    TransferBufferOwnership {
        buffer: &'a Buffer<B>,
        array_element: usize,
        new_queue: QueueType,
        usage_hint: Option<BufferUsage>,
    },
    TransferTextureOwnership {
        texture: &'a Texture<B>,
        array_element: usize,
        base_mip: usize,
        mip_count: usize,
        new_queue: QueueType,
        usage_hint: Option<TextureUsage>,
    },
    TransferCubeMapOwnership {
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        base_mip: usize,
        mip_count: usize,
        face: CubeFace,
        new_queue: QueueType,
        usage_hint: Option<TextureUsage>,
    },
    BindDescriptorSets {
        sets: Vec<&'a DescriptorSet<B>>,
        first: usize,
        stage: ShaderStage,
    },
    BindDescriptorSetsUnchecked {
        sets: Vec<&'a DescriptorSet<B>>,
        first: usize,
        stage: ShaderStage,
    },
    BindVertexBuffers {
        first: usize,
        binds: Vec<VertexBind<'a, B>>,
    },
    BindIndexBuffer {
        buffer: &'a Buffer<B>,
        array_element: usize,
        offset: u64,
        ty: IndexType,
    },
    Scissor {
        attachment: usize,
        scissor: Scissor,
    },
    Draw {
        vertex_count: usize,
        instance_count: usize,
        first_vertex: usize,
        first_instance: usize,
    },
    DrawIndexed {
        index_count: usize,
        instance_count: usize,
        first_index: usize,
        vertex_offset: isize,
        first_instance: usize,
    },
    DrawIndexedIndirect {
        buffer: &'a Buffer<B>,
        array_element: usize,
        offset: u64,
        draw_count: usize,
        stride: u64,
    },
    DrawIndexedIndirectCount {
        draw_buffer: &'a Buffer<B>,
        draw_array_element: usize,
        draw_offset: u64,
        draw_stride: u64,
        count_buffer: &'a Buffer<B>,
        count_array_element: usize,
        count_offset: u64,
        max_draw_count: usize,
    },
    CopyBufferToBuffer(CopyBufferToBuffer<'a, B>),
    CopyBufferToTexture {
        buffer: &'a Buffer<B>,
        texture: &'a Texture<B>,
        copy: BufferTextureCopy,
    },
    CopyTextureToBuffer {
        buffer: &'a Buffer<B>,
        texture: &'a Texture<B>,
        copy: BufferTextureCopy,
    },
    CopyBufferToCubeMap {
        buffer: &'a Buffer<B>,
        cube_map: &'a CubeMap<B>,
        copy: BufferCubeMapCopy,
    },
    CopyCubeMapToBuffer {
        cube_map: &'a CubeMap<B>,
        buffer: &'a Buffer<B>,
        copy: BufferCubeMapCopy,
    },
    Blit {
        src: BlitSource<'a, B>,
        dst: BlitDestination<'a, B>,
        blit: Blit,
        filter: Filter,
    },
    SetTextureUsage {
        tex: &'a Texture<B>,
        new_usage: TextureUsage,
        array_elem: usize,
        base_mip: u32,
        mip_count: usize,
    },
}

/// A command buffer is used to record commands which are the submitted to a queue.
pub struct CommandBuffer<'a, B: Backend> {
    pub(crate) queue_ty: QueueType,
    pub(crate) commands: Vec<Command<'a, B>>,
}

impl<'a, B: Backend> CommandBuffer<'a, B> {
    /// Begins a render pass scope.
    ///
    /// # Arguments
    /// - `descriptor` - A description of the render pass.
    /// - `pass` - A function that records render pass commands.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support graphics
    /// commands.
    ///
    pub fn render_pass(
        &mut self,
        descriptor: RenderPassDescriptor<'a, B>,
        debug_name: Option<&'a str>,
        pass: impl FnOnce(&mut RenderPass<'a, B>),
    ) {
        assert_eq!(
            self.queue_ty,
            QueueType::Main,
            "queue `{:?}` does not support render passes",
            self.queue_ty
        );

        self.commands
            .push(Command::BeginRenderPass(descriptor, debug_name));
        let mut render_pass = RenderPass {
            bound_pipeline: false,
            commands: Vec::default(),
        };
        pass(&mut render_pass);
        self.commands.extend(render_pass.commands);
        self.commands.push(Command::EndRenderPass(debug_name));
    }

    /// Begins a compute pass scope.
    ///
    /// # Arguments
    /// - `pipeline` - The pipeline used for this compute pass.
    /// - `pass` - A function that records compute commands. Returns the work groups for dispatch.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support compute commands.
    pub fn compute_pass(
        &mut self,
        pipeline: &ComputePipeline<B>,
        debug_name: Option<&'a str>,
        pass: impl FnOnce(&mut ComputePass<'a, B>) -> (u32, u32, u32),
    ) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Compute,
            "queue `{:?}` does not support compute passes",
            self.queue_ty
        );

        self.commands
            .push(Command::BeginComputePass(pipeline.clone(), debug_name));
        let mut compute_pass = ComputePass {
            commands: Vec::default(),
        };
        let workgroups = pass(&mut compute_pass);
        self.commands.extend(compute_pass.commands);
        self.commands.push(Command::EndComputePass(
            workgroups.0,
            workgroups.1,
            workgroups.2,
            debug_name,
        ));
    }

    /// Relinquishes ownership of a buffer by the current queue for another queue type.
    ///
    /// # Arguments
    /// - `buffer` - The buffer to transfer ownership of.
    /// - `array_element` - Array element of the buffer to transfer ownership of.
    /// - `new_queue` - The new queue type the buffer will be owned by.
    ///
    /// # Panics
    /// - If the buffer was not created with the queue type provided.
    #[inline(always)]
    pub fn transfer_buffer_ownership(
        &mut self,
        buffer: &'a Buffer<B>,
        array_element: usize,
        new_queue: QueueType,
        usage_hint: Option<BufferUsage>,
    ) {
        assert!(
            buffer.sharing_mode() == SharingMode::Exclusive
                && buffer.queue_types().contains(new_queue.into())
        );
        self.commands.push(Command::TransferBufferOwnership {
            buffer,
            array_element,
            new_queue,
            usage_hint,
        });
    }

    /// Relinquishes ownership of a texture by the current queue for another queue type.
    ///
    /// # Arguments
    /// - `texture` - The texture to transfer ownership of.
    /// - `array_element` - Array element of the texture to transfer ownership of.
    /// - `base_mip` - The base mip level of the array element to transfer ownership of.
    /// - `mip_count` - The number of mips to transfer ownership of.
    /// - `new_queue` - The new queue type the texture will be owned by.
    ///
    /// # Panics
    /// - If the texture was not created with the queue type provided.
    #[inline(always)]
    pub fn transfer_texture_ownership(
        &mut self,
        texture: &'a Texture<B>,
        array_element: usize,
        base_mip: usize,
        mip_count: usize,
        new_queue: QueueType,
        usage_hint: Option<TextureUsage>,
    ) {
        assert!(
            texture.sharing_mode() == SharingMode::Exclusive
                && texture.queue_types().contains(new_queue.into())
        );
        self.commands.push(Command::TransferTextureOwnership {
            texture,
            array_element,
            base_mip,
            mip_count,
            new_queue,
            usage_hint,
        });
    }

    /// Relinquishes ownership of a cube map by the current queue for another queue type.
    ///
    /// # Arguments
    /// - `cube_map` - The cube map to transfer ownership of.
    /// - `array_element` - Array element of the cube map to transfer ownership of.
    /// - `base_mip` - The base mip level of the array element to transfer ownership of.
    /// - `mip_count` - The number of mips to transfer ownership of.
    /// - `face` - The face of the cube map to transfer ownership of.
    /// - `new_queue` - The new queue type the cube map will be owned by.
    ///
    /// # Panics
    /// - If the cube map was not created with the queue type provided.
    #[inline(always)]
    pub fn transfer_cube_map_ownership(
        &mut self,
        cube_map: &'a CubeMap<B>,
        array_element: usize,
        base_mip: usize,
        mip_count: usize,
        face: CubeFace,
        new_queue: QueueType,
        usage_hint: Option<TextureUsage>,
    ) {
        assert!(
            cube_map.sharing_mode() == SharingMode::Exclusive
                && cube_map.queue_types().contains(new_queue.into())
        );
        self.commands.push(Command::TransferCubeMapOwnership {
            cube_map,
            array_element,
            base_mip,
            mip_count,
            face,
            new_queue,
            usage_hint,
        });
    }

    /// Copies data from one buffer into another.
    ///
    /// # Arguments
    /// - `copy` - A description of the copy to perform.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support transfer
    /// commands.
    #[inline(always)]
    pub fn copy_buffer_to_buffer(&mut self, copy: CopyBufferToBuffer<'a, B>) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Transfer,
            "queue `{:?}` does not support transfer commands",
            self.queue_ty
        );
        assert!(copy.dst_offset < copy.dst.size(), "out of bound");
        assert!(copy.src_offset < copy.src.size(), "out of bound");
        assert!(
            copy.len <= copy.dst.size() - copy.dst_offset,
            "attempt to copy too many bytes"
        );
        assert!(
            copy.len <= copy.src.size() - copy.src_offset,
            "attempt to copy too many bytes"
        );
        self.commands.push(Command::CopyBufferToBuffer(copy));
    }

    /// Copies data from a buffer into a texture.
    ///
    /// # Arguments
    /// - `texture` - The destination texture to write to.
    /// - `buffer` - The source buffer to copy from.
    /// - `copy` - A description of the copy to perform.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support transfer
    /// commands.
    #[inline(always)]
    pub fn copy_buffer_to_texture(
        &mut self,
        texture: &'a Texture<B>,
        buffer: &'a Buffer<B>,
        copy: BufferTextureCopy,
    ) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Transfer,
            "queue `{:?}` does not support transfer commands",
            self.queue_ty
        );

        self.commands.push(Command::CopyBufferToTexture {
            buffer,
            texture,
            copy,
        });
    }

    /// Copies data from a texture into a buffer.
    ///
    /// # Arguments
    /// - `buffer` - The destination buffer to write to.
    /// - `texture` The source texture to read from.
    /// - `copy` - A description of the copy to perform.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support transfer
    /// commands.
    #[inline(always)]
    pub fn copy_texture_to_buffer(
        &mut self,
        buffer: &'a Buffer<B>,
        texture: &'a Texture<B>,
        copy: BufferTextureCopy,
    ) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Transfer,
            "queue `{:?}` does not support transfer commands",
            self.queue_ty
        );

        self.commands.push(Command::CopyTextureToBuffer {
            buffer,
            texture,
            copy,
        });
    }

    /// Copies data from a buffer into a cube map.
    ///
    /// # Arguments
    /// - `cube_map` - The destination cube map to write to.
    /// - `buffer` - The source buffer to copy from.
    /// - `copy` - A description of the copy to perform.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support transfer
    /// commands.
    #[inline(always)]
    pub fn copy_buffer_to_cube_map(
        &mut self,
        cube_map: &'a CubeMap<B>,
        buffer: &'a Buffer<B>,
        copy: BufferCubeMapCopy,
    ) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Transfer,
            "queue `{:?}` does not support transfer commands",
            self.queue_ty
        );

        self.commands.push(Command::CopyBufferToCubeMap {
            buffer,
            cube_map,
            copy,
        });
    }

    /// Copies data from a cube map into a buffer.
    /// # Arguments
    /// - `buffer` - The destination buffer to write to.
    /// - `cube_map` - The source cube map to copy from.
    /// - `copy` - A description of the copy to perform.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support transfer
    /// commands.
    #[inline(always)]
    pub fn copy_cube_map_to_buffer(
        &mut self,
        buffer: &'a Buffer<B>,
        cube_map: &'a CubeMap<B>,
        copy: BufferCubeMapCopy,
    ) {
        assert!(
            self.queue_ty == QueueType::Main || self.queue_ty == QueueType::Transfer,
            "queue `{:?}` does not support transfer commands",
            self.queue_ty
        );

        self.commands.push(Command::CopyCubeMapToBuffer {
            buffer,
            cube_map,
            copy,
        });
    }

    /// Copies a region of one texture into another, possibly performing format conversion,
    /// scaling, and filtering.
    ///
    /// # Arguments
    /// - `src` - Source texture.
    /// - `dst` - Destination texture or surface image.
    /// - `blit` - The blit to perform.
    /// - `filter` - Filtering type when the blit requires scaling.
    ///
    /// # Panics
    /// - If the queue type this command buffer was created with does not support graphics
    /// commands.
    #[inline(always)]
    pub fn blit(
        &mut self,
        src: BlitSource<'a, B>,
        dst: BlitDestination<'a, B>,
        blit: Blit,
        filter: Filter,
    ) {
        assert_eq!(
            self.queue_ty,
            QueueType::Main,
            "queue `{:?}` does not support render passes",
            self.queue_ty
        );

        self.commands.push(Command::Blit {
            src,
            dst,
            blit,
            filter,
        });
    }

    /// Prepares a texture to be used in a particular way.
    ///
    /// # Arguments
    /// - `tex` - The texture to prepare.
    /// - `new_usage` - The way the texture will be prepared to use.
    /// - `array_elem` - Array element to prepare.
    /// - `base_mip` - Base mip level to prepare.
    /// - `mip_count` - The number of mips to prepare.
    ///
    /// # Note
    /// Texture usage transitions are performed automatically, so this is almost never needed. The
    /// use case for this is if you're using unsafe commands and might need to manually transition
    /// a usage. Or, you want to perform a usage transition on an asyncronous job.
    #[inline(always)]
    pub fn set_texture_usage(
        &mut self,
        tex: &'a Texture<B>,
        new_usage: TextureUsage,
        array_elem: usize,
        base_mip: u32,
        mip_count: usize,
    ) {
        assert!(new_usage.iter().count() == 1);
        self.commands.push(Command::SetTextureUsage {
            tex,
            new_usage,
            array_elem,
            base_mip,
            mip_count,
        });
    }
}
