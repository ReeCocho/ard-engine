use ::serde::{Deserialize, Serialize};
use ard_math::Vec4;
use bitflags::*;
use half::f16;
use thiserror::*;

use crate::mesh::ObjectBounds;

bitflags! {
    #[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct VertexLayout: u8 {
        const POSITION  = 0b0000_0001;
        const NORMAL    = 0b0000_0010;
        const TANGENT   = 0b0000_0100;
        const UV0       = 0b0000_1000;
        const UV1       = 0b0001_0000;
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VertexAttribute {
    Position,
    Normal,
    Tangent,
    Uv0,
    Uv1,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct VertexData {
    positions: Vec<Vec4>,
    normals: Vec<[i16; 4]>,
    tangents: Vec<[i16; 4]>,
    uv0s: Vec<[f16; 2]>,
    uv1s: Vec<[f16; 2]>,
    bounds: ObjectBounds,
    len: usize,
}

#[derive(Debug, Error, Copy, Clone)]
#[error("vertex layout must have only one bit enabled to be converted to an attribute")]
pub struct VertexLayoutToAttributeError;

impl VertexData {
    pub fn new(len: usize, layout: VertexLayout) -> Self {
        VertexData {
            positions: if layout.contains(VertexLayout::POSITION) {
                vec![Default::default(); len]
            } else {
                Vec::default()
            },
            normals: if layout.contains(VertexLayout::NORMAL) {
                vec![Default::default(); len]
            } else {
                Vec::default()
            },
            tangents: if layout.contains(VertexLayout::TANGENT) {
                vec![Default::default(); len]
            } else {
                Vec::default()
            },
            uv0s: if layout.contains(VertexLayout::UV0) {
                vec![Default::default(); len]
            } else {
                Vec::default()
            },
            uv1s: if layout.contains(VertexLayout::UV1) {
                vec![Default::default(); len]
            } else {
                Vec::default()
            },
            bounds: ObjectBounds::default(),
            len,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn bounds(&self) -> &ObjectBounds {
        &self.bounds
    }

    #[inline(always)]
    pub fn attribute(&self, attr: VertexAttribute) -> &[u8] {
        match attr {
            VertexAttribute::Position => bytemuck::cast_slice(&self.positions),
            VertexAttribute::Normal => bytemuck::cast_slice(&self.normals),
            VertexAttribute::Tangent => bytemuck::cast_slice(&self.tangents),
            VertexAttribute::Uv0 => bytemuck::cast_slice(&self.uv0s),
            VertexAttribute::Uv1 => bytemuck::cast_slice(&self.uv1s),
        }
    }

    #[inline(always)]
    pub fn positions(&self) -> &[Vec4] {
        &self.positions
    }

    #[inline(always)]
    pub fn normals(&self) -> &[[i16; 4]] {
        &self.normals
    }

    #[inline(always)]
    pub fn tangents(&self) -> &[[i16; 4]] {
        &self.tangents
    }

    #[inline(always)]
    pub fn uv0s(&self) -> &[[f16; 2]] {
        &self.uv0s
    }

    #[inline(always)]
    pub fn uv1s(&self) -> &[[f16; 2]] {
        &self.uv1s
    }

    #[inline(always)]
    pub fn positions_mut(&mut self) -> &mut [Vec4] {
        &mut self.positions
    }

    #[inline(always)]
    pub fn normals_mut(&mut self) -> &mut [[i16; 4]] {
        &mut self.normals
    }

    #[inline(always)]
    pub fn tangents_mut(&mut self) -> &mut [[i16; 4]] {
        &mut self.tangents
    }

    #[inline(always)]
    pub fn uv0s_mut(&mut self) -> &mut [[f16; 2]] {
        &mut self.uv0s
    }

    #[inline(always)]
    pub fn uv1s_mut(&mut self) -> &mut [[f16; 2]] {
        &mut self.uv1s
    }

    #[inline(always)]
    pub fn layout(&self) -> VertexLayout {
        let mut out = VertexLayout::empty();

        if !self.positions.is_empty() {
            out |= VertexLayout::POSITION;
        }

        if !self.normals.is_empty() {
            out |= VertexLayout::NORMAL;
        }

        if !self.tangents.is_empty() {
            out |= VertexLayout::TANGENT;
        }

        if !self.uv0s.is_empty() {
            out |= VertexLayout::UV0;
        }

        if !self.uv1s.is_empty() {
            out |= VertexLayout::UV1;
        }

        out
    }

    #[inline]
    pub fn append_from(&mut self, src: &VertexData, src_idx: u32) {
        let src_idx = src_idx as usize;

        if !src.positions.is_empty() {
            self.positions.push(src.positions[src_idx]);
        }

        if !src.normals.is_empty() {
            self.normals.push(src.normals[src_idx]);
        }

        if !src.tangents.is_empty() {
            self.tangents.push(src.tangents[src_idx]);
        }

        if !src.uv0s.is_empty() {
            self.uv0s.push(src.uv0s[src_idx]);
        }

        if !src.uv1s.is_empty() {
            self.uv1s.push(src.uv1s[src_idx]);
        }

        self.len += 1;
    }

    #[inline(always)]
    pub fn compute_bounds(&mut self) {
        self.bounds = ObjectBounds::from_positions(&self.positions);
    }
}

impl VertexAttribute {
    // TODO: Use `std::mem::variant_count` when it comes out of nightly
    pub const COUNT: usize = 5;

    pub const fn size(&self) -> usize {
        match self {
            VertexAttribute::Position => std::mem::size_of::<f32>() * 4,
            VertexAttribute::Normal => std::mem::size_of::<i16>() * 4,
            VertexAttribute::Tangent => std::mem::size_of::<i16>() * 4,
            VertexAttribute::Uv0 => std::mem::size_of::<f16>() * 2,
            VertexAttribute::Uv1 => std::mem::size_of::<f16>() * 2,
        }
    }

    pub const fn idx(&self) -> usize {
        *self as usize
    }
}

impl VertexLayout {
    /// Returns `true` if this vertex layout contains a subset of the vertex components of `other`.
    #[inline(always)]
    pub fn subset_of(&self, other: VertexLayout) -> bool {
        (*self | other) == other
    }
}

impl From<VertexAttribute> for VertexLayout {
    fn from(value: VertexAttribute) -> Self {
        match value {
            VertexAttribute::Position => VertexLayout::POSITION,
            VertexAttribute::Normal => VertexLayout::NORMAL,
            VertexAttribute::Tangent => VertexLayout::TANGENT,
            VertexAttribute::Uv0 => VertexLayout::UV0,
            VertexAttribute::Uv1 => VertexLayout::UV1,
        }
    }
}

impl TryFrom<VertexLayout> for VertexAttribute {
    type Error = VertexLayoutToAttributeError;

    fn try_from(value: VertexLayout) -> Result<Self, Self::Error> {
        match value {
            VertexLayout::POSITION => Ok(VertexAttribute::Position),
            VertexLayout::NORMAL => Ok(VertexAttribute::Normal),
            VertexLayout::TANGENT => Ok(VertexAttribute::Tangent),
            VertexLayout::UV0 => Ok(VertexAttribute::Uv0),
            VertexLayout::UV1 => Ok(VertexAttribute::Uv1),
            _ => Err(VertexLayoutToAttributeError),
        }
    }
}
