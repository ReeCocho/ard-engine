use std::sync::Arc;

use crate::{
    alloc::Buffer,
    camera::meshes::{Block, MeshBuffers},
    prelude::container::EscapeHandle,
    prelude::*,
};
use ard_graphics_api::prelude::*;
use ard_math::*;
use ash::vk;

pub(crate) const MAX_VERTEX_ATTRIBUTE_COUNT: usize = 8;

#[derive(Clone)]
pub struct Mesh {
    pub(crate) id: u32,
    pub(crate) layout_key: VertexLayoutKey,
    pub info: Arc<MeshInfo>,
    pub(crate) escaper: EscapeHandle,
}

/// Type used to pack the flags in a vertex layout into a single value.
pub(crate) type VertexLayoutKey = u8;

/// ## Note
/// The index and vertex buffers are wrapped in `Arcs` to make sure that they exist long enough
/// for the staging buffer to be uploaded.
pub(crate) struct MeshInner {
    pub layout: VertexLayout,
    pub vertex_block: Block,
    pub index_block: Block,
    pub index_count: usize,
    pub vertex_count: usize,
    pub bounds: ObjectBounds,
    /// Indicates that the mesh buffers have been uploaded and the mehs is ready to be used.
    pub ready: bool,
}

pub struct MeshInfo {
    pub bounds: ObjectBounds,
    pub(crate) vertex_block: Block,
    pub(crate) index_block: Block,
    pub index_count: usize,
    pub vertex_count: usize,
}

impl MeshInner {
    /// Creates the interior mesh object and returns two staging buffers. The first is the vertex
    /// buffer and the second is the index buffer. These should be uploaded to the interior buffers
    /// and then destroyed.
    pub unsafe fn new(
        ctx: &GraphicsContext,
        mesh_buffers: &mut MeshBuffers,
        create_info: &MeshCreateInfo,
    ) -> (Self, Buffer, Buffer) {
        assert!(!create_info.indices.is_empty());
        let vertex_count = create_info.positions.len();
        let layout = create_info.vertex_layout();

        // Create vertex staging buffer
        let mut vb_data = Vec::<u8>::default();
        vb_data.extend_from_slice(bytemuck::cast_slice(create_info.positions));

        if let Some(normals) = create_info.normals {
            vb_data.extend_from_slice(bytemuck::cast_slice(normals));
        }

        if let Some(tangents) = create_info.tangents {
            vb_data.extend_from_slice(bytemuck::cast_slice(tangents));
        }

        if let Some(colors) = create_info.colors {
            vb_data.extend_from_slice(bytemuck::cast_slice(colors));
        }

        if let Some(uv0) = create_info.uv0 {
            vb_data.extend_from_slice(bytemuck::cast_slice(uv0));
        }

        if let Some(uv1) = create_info.uv1 {
            vb_data.extend_from_slice(bytemuck::cast_slice(uv1));
        }

        if let Some(uv2) = create_info.uv2 {
            vb_data.extend_from_slice(bytemuck::cast_slice(uv2));
        }

        if let Some(uv3) = create_info.uv3 {
            vb_data.extend_from_slice(bytemuck::cast_slice(uv3));
        }

        let vb_staging = Buffer::new_staging_buffer(ctx, &vb_data);

        // Create index staging buffer
        let mut ib_data =
            Vec::<u8>::with_capacity(std::mem::size_of::<u16>() * create_info.indices.len());
        ib_data.extend_from_slice(bytemuck::cast_slice(create_info.indices));
        let ib_staging = Buffer::new_staging_buffer(ctx, &ib_data);

        // Allocate block for vertex data
        let vbs = mesh_buffers.get_vertex_buffer(&layout);
        let vertex_block = if let Some(block) = vbs.allocate(vertex_count) {
            block
        }
        // Not enough room. We must expand the buffer
        else {
            // Wait for all GPU work to finish
            ctx.0.device.device_wait_idle().unwrap();

            // Perform expansion
            let (pool, commands) = ctx
                .0
                .create_single_use_pool(ctx.0.queue_family_indices.transfer);
            let _buffers = vbs.expand_for(commands, vertex_count);
            ctx.0.submit_single_use_pool(ctx.0.transfer, pool, commands);

            // If allocation fails now, something very bad has happened
            vbs.allocate(vertex_count)
                .expect("expanded vertex buffer but still couldn't allocate")
        };

        // Allocate block for index data
        let ib = mesh_buffers.get_index_buffer();
        let index_block = if let Some(block) = ib.allocate(create_info.indices.len()) {
            block
        }
        // Not enough room. We must expand the buffer
        else {
            // Wait for all GPU work to finish
            ctx.0.device.device_wait_idle().unwrap();

            // Perform expansion
            let (pool, commands) = ctx
                .0
                .create_single_use_pool(ctx.0.queue_family_indices.transfer);
            let _buffers = ib.expand_for(create_info.indices.len(), commands);
            ctx.0.submit_single_use_pool(ctx.0.transfer, pool, commands);

            // If allocation fails now, something very bad has happened
            ib.allocate(create_info.indices.len())
                .expect("expanded index buffer but still couldn't allocate")
        };

        (
            MeshInner {
                layout,
                vertex_block,
                index_block,
                index_count: create_info.indices.len(),
                vertex_count,
                bounds: create_info.bounds(),
                ready: false,
            },
            vb_staging,
            ib_staging,
        )
    }
}

impl MeshApi for Mesh {
    fn index_count(&self) -> usize {
        self.info.index_count
    }

    fn vertex_count(&self) -> usize {
        self.info.vertex_count
    }
}

pub(crate) fn bindings_of(layout: &VertexLayout) -> Vec<vk::VertexInputBindingDescription> {
    let mut bindings = vec![vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(std::mem::size_of::<Vec4>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build()];

    if layout.normals {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec4>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.tangents {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec4>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.colors {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec4>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.uv0 {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec2>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.uv1 {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec2>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.uv2 {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec2>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    if layout.uv3 {
        bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(bindings.len() as u32)
                .stride(std::mem::size_of::<Vec2>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );
    }

    bindings
}

pub(crate) fn attributes_of(layout: &VertexLayout) -> Vec<vk::VertexInputAttributeDescription> {
    let mut base_location = 0;
    let mut attributes = vec![vk::VertexInputAttributeDescription::builder()
        .binding(base_location)
        .location(base_location)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(0)
        .build()];

    base_location += 1;

    if layout.normals {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.tangents {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.colors {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.uv0 {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.uv1 {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.uv2 {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(),
        );

        base_location += 1;
    }

    if layout.uv3 {
        attributes.push(
            vk::VertexInputAttributeDescription::builder()
                .binding(base_location)
                .location(base_location)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(),
        );
    }

    attributes
}
