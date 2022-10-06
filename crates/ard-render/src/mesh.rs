use ard_log::warn;
use ard_math::{Vec2, Vec4};
use ard_pal::prelude::*;
use bitflags::bitflags;

use crate::factory::{
    allocator::{EscapeHandle, ResourceId},
    meshes::{MeshBlock, MeshBuffers},
};

#[derive(Clone)]
pub struct Mesh {
    pub(crate) id: ResourceId,
    pub(crate) layout: VertexLayout,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct MeshInner {
    pub layout: VertexLayout,
    pub bounds: ObjectBounds,
    pub vertex_block: MeshBlock,
    pub index_block: MeshBlock,
    pub index_count: usize,
    pub vertex_count: usize,
    /// Indicates that the mesh buffers have been uploaded and the mesh is ready to be used.
    pub ready: bool,
}

bitflags! {
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttributeType {
    Position,
    Normal,
    Tangent,
    Color,
    Uv0,
    Uv1,
    Uv2,
    Uv3,
}

/// Volume bounded by the dimensions of a box and sphere.
#[derive(Debug, Default, Copy, Clone)]
#[repr(C)]
pub struct ObjectBounds {
    /// `w` component of `center` should be a bounding sphere radius.
    pub center: Vec4,
    pub half_extents: Vec4,
}

#[derive(Default)]
pub struct MeshCreateInfo<'a> {
    pub bounds: MeshBounds,
    pub indices: &'a [u32],
    pub positions: &'a [Vec4],
    /// If `Some`, must be the same length as `positions`.
    pub normals: Option<&'a [Vec4]>,
    /// If `Some`, must be the same length as `positions`.
    pub tangents: Option<&'a [Vec4]>,
    /// If `Some`, must be the same length as `positions`.
    pub colors: Option<&'a [Vec4]>,
    /// If `Some`, must be the same length as `positions`.
    pub uv0: Option<&'a [Vec2]>,
    /// If `Some`, must be the same length as `positions`.
    pub uv1: Option<&'a [Vec2]>,
    /// If `Some`, must be the same length as `positions`.
    pub uv2: Option<&'a [Vec2]>,
    /// If `Some`, must be the same length as `positions`.
    pub uv3: Option<&'a [Vec2]>,
}

/// Object bounds for a mesh.
#[derive(Debug, Copy, Clone)]
pub enum MeshBounds {
    /// Manually set the bounds.
    Manual(ObjectBounds),
    /// Bounds are autogenerated from the positions list.
    Generate,
}

impl MeshInner {
    pub fn new(
        ctx: &Context,
        mesh_buffers: &mut MeshBuffers,
        create_info: MeshCreateInfo,
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

        let vb_staging =
            Buffer::new_staging(ctx.clone(), Some(String::from("vertex_staging")), &vb_data)
                .unwrap();

        // Create index staging buffer
        let mut ib_data =
            Vec::<u8>::with_capacity(std::mem::size_of::<u32>() * create_info.indices.len());
        ib_data.extend_from_slice(bytemuck::cast_slice(create_info.indices));
        let ib_staging =
            Buffer::new_staging(ctx.clone(), Some(String::from("index_staging")), &ib_data)
                .unwrap();

        // Allocate block for vertex data
        let vbs = mesh_buffers.get_vertex_buffer_mut(layout);
        let vertex_block = match vbs.allocate(vertex_count) {
            Some(block) => block,
            // Not enough room we mut expand the buffer
            None => {
                warn!("Vertex buffer expanded. Consider making the default size larger.");
                vbs.expand_for(ctx, vertex_count);
                vbs.allocate(vertex_count).unwrap()
            }
        };

        // Allocate block for index data
        let ib = mesh_buffers.get_index_buffer_mut();
        let index_block = match ib.allocate(create_info.indices.len()) {
            Some(block) => block,
            // Not enough room we mut expand the buffer
            None => {
                warn!("Index buffer expanded. Consider making the default size larger.");
                ib.expand_for(ctx, create_info.indices.len());
                ib.allocate(create_info.indices.len()).unwrap()
            }
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

impl<'a> MeshCreateInfo<'a> {
    /// Get the bounds for the positions contained.
    pub fn bounds(&self) -> ObjectBounds {
        if let MeshBounds::Manual(bounds) = self.bounds {
            return bounds;
        }

        if self.positions.is_empty() {
            return ObjectBounds::default();
        }

        let mut min = self.positions[0];
        let mut max = self.positions[0];
        let mut sqr_radius = min.x.powi(2) + min.z.powi(2) + min.y.powi(2);

        for position in self.positions {
            let new_sqr_radius = position.x.powi(2) + position.z.powi(2) + position.y.powi(2);

            if new_sqr_radius > sqr_radius {
                sqr_radius = new_sqr_radius;
            }

            if position.x < min.x {
                min.x = position.x;
            }

            if position.y < min.y {
                min.y = position.y;
            }

            if position.z < min.z {
                min.z = position.z;
            }

            if position.x > max.x {
                max.x = position.x;
            }

            if position.y > max.y {
                max.y = position.y;
            }

            if position.z > max.z {
                max.z = position.z;
            }
        }

        ObjectBounds {
            center: Vec4::new(
                (max.x + min.x) / 2.0,
                (max.y + min.y) / 2.0,
                (max.z + min.z) / 2.0,
                sqr_radius.sqrt(),
            ),
            half_extents: Vec4::new(
                (max.x - min.x) / 2.0,
                (max.y - min.y) / 2.0,
                (max.z - min.z) / 2.0,
                1.0,
            ),
        }
    }

    #[inline(always)]
    pub fn vertex_layout(&self) -> VertexLayout {
        let mut layout = VertexLayout::empty();
        if self.normals.is_some() {
            layout |= VertexLayout::NORMAL;
        }
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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
        } else {
            state.bindings.push(VertexInputBinding {
                binding: state.bindings.len() as u32,
                stride: 0,
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

impl Default for MeshBounds {
    #[inline(always)]
    fn default() -> Self {
        MeshBounds::Manual(ObjectBounds::default())
    }
}
