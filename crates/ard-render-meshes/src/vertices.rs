use ard_formats::{
    mesh::{MeshData, MeshDataBuilder},
    vertex::VertexLayout,
};
use ard_math::{Vec2, Vec4};

/// Separated vertex attributes. All slices must be equal lengths.
#[derive(Debug)]
pub struct VertexAttributes<'a> {
    pub indices: &'a [u32],
    pub positions: &'a [Vec4],
    pub normals: &'a [Vec4],
    pub tangents: Option<&'a [Vec4]>,
    pub uv0: Option<&'a [Vec2]>,
    pub uv1: Option<&'a [Vec2]>,
}

impl<'a> From<VertexAttributes<'a>> for MeshData {
    fn from(value: VertexAttributes<'a>) -> Self {
        let mut builder =
            MeshDataBuilder::new(value.layout(), value.vertex_count(), value.index_count())
                .add_indices(value.indices)
                .add_positions(value.positions)
                .add_vec4_normals(value.normals);

        if let Some(tangents) = value.tangents {
            builder = builder.add_vec4_tangents(tangents);
        }

        if let Some(uv0) = value.uv0 {
            builder = builder.add_vec2_uvs(uv0, 0);
        }

        if let Some(uv1) = value.uv1 {
            builder = builder.add_vec2_uvs(uv1, 1);
        }

        builder.build()
    }
}

impl<'a> VertexAttributes<'a> {
    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    #[inline(always)]
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    #[inline(always)]
    pub fn layout(&self) -> VertexLayout {
        let mut layout = VertexLayout::POSITION | VertexLayout::NORMAL;

        if self.tangents.is_some() {
            layout |= VertexLayout::TANGENT;
        }
        if self.uv0.is_some() {
            layout |= VertexLayout::UV0;
        }
        if self.uv1.is_some() {
            layout |= VertexLayout::UV1;
        }

        layout
    }
}
