use ard_math::*;
use ard_pal::prelude::*;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshHeader {
    pub index_count: u32,
    pub vertex_count: u32,
    pub vertex_layout: VertexLayout,
}

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct VertexLayout: u8 {
        const NORMAL    = 0b0000_0001;
        const TANGENT   = 0b0000_0010;
        const COLOR     = 0b0000_0100;
        const UV0       = 0b0000_1000;
        const UV1       = 0b0001_0000;
        const UV2       = 0b0010_0000;
        const UV3       = 0b0100_0000;
    }
}

impl VertexLayout {
    /// Returns `true` if this vertex layout contains a subset of the vertex components of `other`.
    #[inline(always)]
    pub fn subset_of(&self, other: &VertexLayout) -> bool {
        (*self | *other) == *other
    }

    pub fn vertex_input_state(&self) -> VertexInputState {
        let mut state = VertexInputState {
            attributes: Vec::with_capacity(8),
            bindings: Vec::with_capacity(8),
            topology: PrimitiveTopology::TriangleList,
        };

        state.bindings.push(VertexInputBinding {
            binding: 0,
            stride: std::mem::size_of::<Vec4>() as u32,
            input_rate: VertexInputRate::Vertex,
        });
        state.attributes.push(VertexInputAttribute {
            binding: 0,
            location: 0,
            format: VertexFormat::XyzwF32,
            offset: 0,
        });

        if self.contains(VertexLayout::NORMAL) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec4>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyzwF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::TANGENT) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec4>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyzwF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::COLOR) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec4>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyzwF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::UV0) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec2>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::UV1) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec2>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::UV2) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec2>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyF32,
                offset: 0,
            });
        }

        if self.contains(VertexLayout::UV3) {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: std::mem::size_of::<Vec2>() as u32,
                input_rate: VertexInputRate::Vertex,
            });
            state.attributes.push(VertexInputAttribute {
                binding: state.attributes.len() as u32,
                location: state.attributes.len() as u32,
                format: VertexFormat::XyF32,
                offset: 0,
            });
        }

        state
    }
}
