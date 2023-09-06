use ard_ecs::prelude::Component;
use ard_formats::mesh::{IndexSource, VertexLayout, VertexSource};
use ard_pal::prelude::{BufferCreateError, Context};
use ard_render_base::resource::{ResourceHandle, ResourceId};
use thiserror::Error;

use crate::factory::{MeshBlock, MeshFactory, MeshUpload};

pub struct MeshCreateInfo<V, I> {
    pub debug_name: Option<String>,
    pub vertices: V,
    pub indices: I,
}

#[derive(Debug, Error)]
pub enum MeshCreateError<V: VertexSource, I: IndexSource> {
    #[error("no vertices provided")]
    NoVertices,
    #[error("no indices provided")]
    NoIndices,
    #[error("vertex data error: {0}")]
    VertexDataErr(V::Error),
    #[error("index data error: {0}")]
    IndexDataErr(I::Error),
    #[error("gpu error: {0}")]
    GpuError(BufferCreateError),
}

#[derive(Clone, Component)]
pub struct Mesh {
    layout: VertexLayout,
    handle: ResourceHandle,
}

pub struct MeshResource {
    pub block: MeshBlock,
    pub index_count: usize,
    pub vertex_count: usize,
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
    pub fn new<V: VertexSource, I: IndexSource>(
        create_info: MeshCreateInfo<V, I>,
        ctx: &Context,
        factory: &mut MeshFactory,
    ) -> Result<(Self, MeshUpload), MeshCreateError<V, I>> {
        if create_info.indices.index_count() == 0 {
            return Err(MeshCreateError::NoIndices);
        }

        if create_info.vertices.vertex_count() == 0 {
            return Err(MeshCreateError::NoVertices);
        }

        // Convert sources into raw data
        let vertex_data = match create_info.vertices.into_vertex_data() {
            Ok(vd) => vd,
            Err(err) => return Err(MeshCreateError::VertexDataErr(err)),
        };

        let index_data = match create_info.indices.into_index_data() {
            Ok(vd) => vd,
            Err(err) => return Err(MeshCreateError::IndexDataErr(err)),
        };

        // Create staging buffers
        let vertex_staging = vertex_data.staging_buffer(
            ctx.clone(),
            Some(format!("vertex_stating({:?})", &create_info.debug_name)),
        )?;
        let index_staging = index_data.staging_buffer(
            ctx.clone(),
            Some(format!("index_stating({:?})", &create_info.debug_name)),
        )?;

        // Allocate a slot for the mesh in the factory
        let block = factory.allocate(vertex_data.layout(), vertex_data.len(), index_data.len());

        Ok((
            MeshResource {
                block,
                index_count: index_data.len(),
                vertex_count: vertex_data.len(),
                ready: false,
            },
            MeshUpload {
                vertex_staging,
                vertex_offsets: vertex_data.offsets().clone(),
                index_staging,
                vertex_count: vertex_data.len(),
                block,
            },
        ))
    }
}

impl<V: VertexSource, I: IndexSource> From<BufferCreateError> for MeshCreateError<V, I> {
    fn from(value: BufferCreateError) -> Self {
        MeshCreateError::GpuError(value)
    }
}
