use ard_formats::mesh::{VertexData, VertexDataBuilder, VertexLayout, VertexSource};
use ard_math::{Vec2, Vec4};
use thiserror::Error;

/// Separated vertex attributes. All slices must be equal lengths.
#[derive(Debug)]
pub struct VertexAttributes<'a> {
    pub positions: &'a [Vec4],
    pub normals: &'a [Vec4],
    pub tangents: Option<&'a [Vec4]>,
    pub colors: Option<&'a [Vec4]>,
    pub uv0: Option<&'a [Vec2]>,
    pub uv1: Option<&'a [Vec2]>,
    pub uv2: Option<&'a [Vec2]>,
    pub uv3: Option<&'a [Vec2]>,
}

#[derive(Debug, Error)]
#[error("vertex attribute lengths did not match")]
pub struct VertexAttributeMismatchingLen;

impl<'a> VertexSource for VertexAttributes<'a> {
    type Error = VertexAttributeMismatchingLen;

    fn into_vertex_data(self) -> Result<VertexData, Self::Error> {
        let len = self.positions.len();
        if self.normals.len() != len {
            return Err(VertexAttributeMismatchingLen);
        }

        let mut builder = VertexDataBuilder::new(self.layout(), self.positions.len())
            .add_positions(self.positions)
            .add_vec4_normals(self.normals);

        if let Some(tangents) = &self.tangents {
            builder = builder.add_vec4_tangents(tangents);
            if tangents.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        if let Some(colors) = &self.colors {
            builder = builder.add_vec4_colors(colors);
            if colors.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        if let Some(uv) = &self.uv0 {
            builder = builder.add_vec2_uvs(uv, 0);
            if uv.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        if let Some(uv) = &self.uv1 {
            builder = builder.add_vec2_uvs(uv, 1);
            if uv.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        if let Some(uv) = &self.uv2 {
            builder = builder.add_vec2_uvs(uv, 2);
            if uv.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        if let Some(uv) = &self.uv3 {
            builder = builder.add_vec2_uvs(uv, 3);
            if uv.len() != len {
                return Err(VertexAttributeMismatchingLen);
            }
        }

        Ok(builder.build())
    }

    #[inline(always)]
    fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    #[inline(always)]
    fn layout(&self) -> VertexLayout {
        let mut layout = VertexLayout::POSITION | VertexLayout::NORMAL;

        if self.tangents.is_some() {
            layout |= VertexLayout::TANGENT;
        }
        if self.colors.is_some() {
            layout |= VertexLayout::COLOR;
        }
        if self.uv0.is_some() {
            layout |= VertexLayout::UV0;
        }
        if self.uv1.is_some() {
            layout |= VertexLayout::UV1;
        }
        if self.uv2.is_some() {
            layout |= VertexLayout::UV2;
        }
        if self.uv3.is_some() {
            layout |= VertexLayout::UV3;
        }

        layout
    }
}
