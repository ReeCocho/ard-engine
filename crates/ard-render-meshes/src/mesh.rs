use std::ops::DerefMut;

use ard_ecs::prelude::Component;
use ard_formats::{
    mesh::{MeshData, ObjectBounds},
    meshlet::Meshlet,
    vertex::VertexLayout,
};
use ard_math::{UVec4, Vec4Swizzles};
use ard_pal::prelude::*;
use ard_render_base::resource::{ResourceHandle, ResourceId};
use ard_render_si::types::{GpuMeshInfo, GpuMeshlet, GpuObjectBounds};
use thiserror::Error;

use crate::factory::{MeshBlock, MeshFactory, MeshUpload};

pub struct MeshCreateInfo<M> {
    pub debug_name: Option<String>,
    pub data: M,
}

#[derive(Debug, Error)]
pub enum MeshCreateError {
    #[error("no vertices provided")]
    NoVertices,
    #[error("no indices provided")]
    NoIndices,
    #[error("no meshlets provided")]
    NoMeshlets,
    #[error("gpu error: {0}")]
    GpuError(BufferCreateError),
}

#[derive(Clone, Component)]
pub struct Mesh {
    layout: VertexLayout,
    handle: ResourceHandle,
}

#[derive(Debug)]
pub struct MeshResource {
    pub block: MeshBlock,
    pub bounds: ObjectBounds,
    pub index_count: usize,
    pub vertex_count: usize,
    pub meshlet_count: usize,
    /// Indicates tht the mesh has been uploaded to the GPU and is ready to be rendered.
    pub ready: bool,
}

impl Mesh {
    pub fn new(handle: ResourceHandle, layout: VertexLayout) -> Self {
        Mesh { layout, handle }
    }

    #[inline(always)]
    pub fn layout(&self) -> VertexLayout {
        self.layout
    }

    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.handle.id()
    }
}

impl MeshResource {
    pub fn new<M: Into<MeshData>>(
        create_info: MeshCreateInfo<M>,
        ctx: &Context,
        factory: &mut MeshFactory,
    ) -> Result<(Self, MeshUpload, GpuMeshInfo), MeshCreateError> {
        let data: MeshData = create_info.data.into();

        if data.index_count() == 0 {
            return Err(MeshCreateError::NoIndices);
        }

        if data.vertex_count() == 0 {
            return Err(MeshCreateError::NoVertices);
        }

        if data.meshlet_count() == 0 {
            return Err(MeshCreateError::NoMeshlets);
        }

        // Allocate a slot for the mesh in the factory
        let block = factory.allocate(
            data.layout(),
            data.vertex_count(),
            data.index_count(),
            data.meshlet_count(),
        );

        // Create staging buffers
        let (vertex_staging, vertex_offsets) = data.vertex_staging(ctx);
        let index_staging = data.index_staging(ctx, block.vertex_block().base());
        let meshlet_staging = Self::meshlet_staging(ctx, data.bounds(), &block, data.meshlets());

        let bounds = *data.bounds();

        Ok((
            MeshResource {
                block,
                index_count: data.index_count(),
                vertex_count: data.vertex_count(),
                meshlet_count: data.meshlet_count(),
                bounds,
                ready: false,
            },
            MeshUpload {
                vertex_staging,
                vertex_offsets,
                index_staging,
                vertex_count: data.vertex_count(),
                block,
                meshlet_staging,
                meshlet_count: data.meshlet_count(),
            },
            GpuMeshInfo {
                bounds: GpuObjectBounds {
                    min_pt: bounds.min_pt,
                    max_pt: bounds.max_pt,
                },
                first_index: block.index_block().base(),
                vertex_offset: block.vertex_block().base() as i32,
                index_count: data.index_count() as u32,
            },
        ))
    }

    fn meshlet_staging(
        ctx: &Context,
        obj_bounds: &ObjectBounds,
        block: &MeshBlock,
        meshlets: &[Meshlet],
    ) -> Buffer {
        let mut staging = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (meshlets.len() * std::mem::size_of::<GpuMeshlet>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("vertex_staging".into()),
            },
        )
        .unwrap();

        let obj_range = obj_bounds.max_pt.xyz() - obj_bounds.min_pt.xyz();

        let mut view = staging.write(0).unwrap();
        let meshlet_slice = bytemuck::cast_slice_mut::<_, GpuMeshlet>(view.deref_mut());

        meshlets.iter().enumerate().for_each(|(i, meshlet)| {
            // Meshlet bounds relative to object bounds
            let min_pt = (meshlet.bounds.min_pt.xyz() - obj_bounds.min_pt.xyz()) / obj_range;
            let max_pt = (meshlet.bounds.max_pt.xyz() - obj_bounds.min_pt.xyz()) / obj_range;

            // Refer to "ard_render_si types" for the layout of this structure.
            let mut data = UVec4::ZERO;
            data.x = block.vertex_block().base() + meshlet.vertex_offset;
            data.y = block.index_block().base() + meshlet.index_offset;
            data.z = meshlet.vertex_count as u32
                | ((meshlet.primitive_count as u32) << 8)
                | (f32_to_unorm8_floor(min_pt.x) << 16)
                | (f32_to_unorm8_floor(min_pt.y) << 24);
            data.w = f32_to_unorm8_floor(min_pt.z)
                | (f32_to_unorm8_ceil(max_pt.x) << 8)
                | (f32_to_unorm8_ceil(max_pt.y) << 16)
                | (f32_to_unorm8_ceil(max_pt.z) << 24);

            meshlet_slice[i] = GpuMeshlet { data }
        });

        std::mem::drop(view);

        staging
    }
}

#[inline(always)]
fn f32_to_unorm8_floor(mut v: f32) -> u32 {
    v = v.clamp(0.0, 1.0);
    (v * 255.0).floor() as u32
}

#[inline(always)]
fn f32_to_unorm8_ceil(mut v: f32) -> u32 {
    v = v.clamp(0.0, 1.0);
    (v * 255.0).ceil() as u32
}
