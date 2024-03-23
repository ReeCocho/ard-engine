use std::{collections::HashMap, ops::DerefMut, path::PathBuf};

use ard_math::*;
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use half::f16;
use serde::{Deserialize, Serialize};

use crate::{
    meshlet::{MeshClustifier, Meshlet},
    vertex::{VertexAttribute, VertexData, VertexLayout},
};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshHeader {
    pub index_count: u32,
    pub vertex_count: u32,
    pub meshlet_count: u32,
    pub vertex_layout: VertexLayout,
}

#[derive(Serialize, Deserialize)]
pub struct MeshData {
    vertices: VertexData,
    indices: Vec<u8>,
    meshlets: Vec<Meshlet>,
}

pub struct MeshDataBuilder {
    index_data: Vec<u32>,
    vertex_data: VertexData,
}

/// Volume bounded by the dimensions of a box and sphere.
#[derive(Debug, Serialize, Deserialize, Default, Copy, Clone)]
#[repr(C)]
pub struct ObjectBounds {
    /// `w` component of `min_pt` should be a bounding sphere radius.
    pub min_pt: Vec4,
    pub max_pt: Vec4,
}

unsafe impl Pod for ObjectBounds {}
unsafe impl Zeroable for ObjectBounds {}

impl MeshData {
    pub const INDEX_TYPE: IndexType = IndexType::U32;
    pub const INDEX_SIZE: usize = std::mem::size_of::<u32>();

    #[inline(always)]
    pub fn layout(&self) -> VertexLayout {
        self.vertices.layout()
    }

    #[inline(always)]
    pub fn meshlets(&self) -> &[Meshlet] {
        &self.meshlets
    }

    #[inline(always)]
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[inline(always)]
    pub fn meshlet_count(&self) -> usize {
        self.meshlets.len()
    }

    #[inline(always)]
    pub fn bounds(&self) -> &ObjectBounds {
        self.vertices.bounds()
    }

    pub fn vertex_staging(&self, ctx: &Context) -> (Buffer, HashMap<VertexAttribute, u32>) {
        let layout = self.vertices.layout();

        // Compute offsets for each attribute type.
        let mut cur_offset = 0;
        let mut offsets = HashMap::default();

        for bit in VertexLayout::all().iter() {
            let attr = VertexAttribute::try_from(bit).unwrap();
            if layout.contains(bit) {
                offsets.insert(attr, cur_offset);
                cur_offset += (attr.size() * self.vertices.len()) as u32;
            }
        }

        // Create a staging buffer to accomodate the vertex data.
        let mut staging = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: cur_offset as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("vertex_staging".into()),
            },
        )
        .unwrap();

        // Copy in attributes
        let mut view = staging.write(0).unwrap();
        offsets.iter().for_each(|(attr, offset)| {
            let rng = (*offset as usize)..(*offset as usize + attr.size() * self.vertices.len());
            view[rng].copy_from_slice(self.vertices.attribute(*attr));
        });

        std::mem::drop(view);

        (staging, offsets)
    }

    pub fn index_staging(&self, ctx: &Context, global_vertex_offset: u32) -> Buffer {
        let mut staging = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (self.indices.len() * MeshData::INDEX_SIZE) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::TRANSFER_SRC,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("index_staging".into()),
            },
        )
        .unwrap();

        // Copy in indices
        let mut view = staging.write(0).unwrap();
        let idx_slice = bytemuck::cast_slice_mut::<_, u32>(view.deref_mut());

        // Loop over every meshlet
        self.meshlets.iter().for_each(|meshlet| {
            // Loop over every index in the meshlet and write in the global index
            let index_count = meshlet.primitive_count as usize * 3;
            for i in 0..index_count {
                let src_idx = meshlet.index_offset as usize + i;
                let meshlet_rel_idx = self.indices[src_idx] as u32;
                idx_slice[src_idx] = global_vertex_offset + meshlet.vertex_offset + meshlet_rel_idx;
            }
        });

        std::mem::drop(view);

        staging
    }
}

impl MeshDataBuilder {
    pub fn new(layout: VertexLayout, vertex_count: usize, index_count: usize) -> Self {
        Self {
            vertex_data: VertexData::new(vertex_count, layout),
            index_data: vec![0; index_count],
        }
    }

    pub fn add_indices(mut self, indices: &[u32]) -> Self {
        assert_eq!(indices.len(), self.index_data.len());
        self.index_data.copy_from_slice(indices);
        self
    }

    pub fn add_positions(mut self, src: &[Vec4]) -> Self {
        assert_eq!(src.len(), self.vertex_data.len());
        self.vertex_data
            .positions_mut()
            .iter_mut()
            .zip(src.iter())
            .for_each(|(dst, src)| {
                *dst = Vec4::from((src.xyz(), 1.0));
            });
        self.vertex_data.compute_bounds();
        self
    }

    pub fn add_vec4_normals(mut self, src: &[Vec4]) -> Self {
        assert_eq!(src.len(), self.vertex_data.len());
        self.vertex_data
            .normals_mut()
            .iter_mut()
            .zip(src.iter())
            .for_each(|(dst, src)| {
                *dst = Self::vec3_vector_to_16snorm(src.xyz());
            });
        self
    }

    pub fn add_vec4_tangents(mut self, src: &[Vec4]) -> Self {
        assert_eq!(src.len(), self.vertex_data.len());
        self.vertex_data
            .tangents_mut()
            .iter_mut()
            .zip(src.iter())
            .for_each(|(dst, src)| {
                *dst = Self::vec3_vector_to_16snorm(src.xyz());
            });
        self
    }

    pub fn add_vec2_uvs(mut self, src: &[Vec2], idx: usize) -> Self {
        assert_eq!(src.len(), self.vertex_data.len());
        let attribute = match idx {
            0 => self.vertex_data.uv0s_mut(),
            1 => self.vertex_data.uv1s_mut(),
            _ => return self,
        };
        attribute.iter_mut().zip(src.iter()).for_each(|(dst, src)| {
            dst[0] = f16::from_f32(src.x);
            dst[1] = f16::from_f32(src.y);
        });
        self
    }

    pub fn build(self) -> MeshData {
        let res = MeshClustifier::new(self.vertex_data, self.index_data).build();
        MeshData {
            vertices: res.vertices,
            indices: res.indices,
            meshlets: res.meshlets,
        }
    }

    /// Convert a normalized vector to a signed 16-bit per channel value packed into a u64.
    fn vec3_vector_to_16snorm(vec: Vec3) -> [i16; 4] {
        let vec = vec.try_normalize().unwrap_or(Vec3::Z);

        let x = (vec.x * 32727.0).round().clamp(-32727.0, 32727.0) as i16;
        let y = (vec.y * 32727.0).round().clamp(-32727.0, 32727.0) as i16;
        let z = (vec.z * 32727.0).round().clamp(-32727.0, 32727.0) as i16;

        [x, y, z, 0]
    }
}

impl MeshHeader {
    pub fn mesh_data_path(root: impl Into<PathBuf>) -> PathBuf {
        let mut path: PathBuf = root.into();
        path.push("data");
        path
    }
}

impl ObjectBounds {
    pub fn from_positions(src: &[Vec4]) -> Self {
        if src.is_empty() {
            return ObjectBounds::default();
        }

        let mut min = src[0];
        let mut max = src[0];
        let mut sqr_radius = min.x.powi(2) + min.z.powi(2) + min.y.powi(2);

        for position in src {
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
            min_pt: Vec4::from((min.xyz(), sqr_radius.sqrt())),
            max_pt: Vec4::from((max.xyz(), 0.0)),
        }
    }
}
