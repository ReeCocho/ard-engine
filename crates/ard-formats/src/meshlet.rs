use ard_math::*;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{mesh::ObjectBounds, vertex::VertexData};

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct Meshlet {
    pub vertex_offset: u32,
    pub index_offset: u32,
    pub bounds: ObjectBounds,
    pub vertex_count: u8,
    pub primitive_count: u8,
}

impl Meshlet {
    const MAX_VERTICES: usize = 64;
    const MAX_PRIMITIVES: usize = 126;
}

pub struct MeshClustifier {
    vertices: VertexData,
    indices: Vec<u32>,
    unused_triangles: Vec<Triangle>,
    used_tri_lookup: Vec<bool>,
    meshlet: WorkingMeshlet,
    /// Neighbour triangles of each triangle.
    triangle_neighbours: Vec<Vec<Triangle>>,
}

pub struct MeshClustifierOutput {
    /// Resulting vertices after clustering. Note that some vertices may end up being duplicated
    /// from the original mesh.
    pub vertices: VertexData,
    /// Indices for the meshlets. Each meshlet holds an offset into this list such that:
    ///
    /// `final_index = base_offset_of_mesh + base_offset_of_meshlet + index`
    ///
    /// This final index should be computed at upload time to minimize the amount of work needed
    /// in the mesh shader, and to allow for these indices to be reused in ray tracing.
    ///
    /// We only need 8 bits per index since each meshlet has at most `Meshlet::MAX_VERTICES` which
    /// should always be less than `256`.
    pub indices: Vec<u8>,
    /// The generated meshlets.
    pub meshlets: Vec<Meshlet>,
}

#[derive(Default, Clone)]
struct WorkingMeshlet {
    vertices: Vec<u32>,
    triangles: Vec<Triangle>,
    border: Vec<Triangle>,
    center: Vec3A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Triangle(u32);

impl Triangle {
    #[inline(always)]
    fn new(id: u32) -> Self {
        Self(id)
    }
}

impl WorkingMeshlet {
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.triangles.is_empty() && self.vertices.is_empty()
    }

    #[inline]
    fn add(
        &mut self,
        tri: Triangle,
        indices: &[u32],
        positions: &[Vec4],
        triangle_neighbours: &[Vec<Triangle>],
    ) -> bool {
        // Skip if we already contain this triangle
        if self.triangles.contains(&tri) {
            return false;
        }

        // Skip if we're at our primitive limit
        if self.triangles.len() == Meshlet::MAX_PRIMITIVES {
            return false;
        }

        // Get the indices of the triangle
        let base = tri.0 as usize * 3;
        let inds = [indices[base], indices[base + 1], indices[base + 2]];

        // Find out how many vertices are new
        let new_verts: usize = inds
            .iter()
            .map(|i| !self.vertices.contains(i) as usize)
            .sum();

        // Don't add if we couldn't hold the new vertices
        if self.vertices.len() + new_verts > Meshlet::MAX_VERTICES {
            return false;
        }

        // Insert and recompute border
        if !self.triangles.contains(&tri) {
            self.triangles.push(tri);
        }
        if !self.vertices.contains(&inds[0]) {
            self.vertices.push(inds[0]);
        }
        if !self.vertices.contains(&inds[1]) {
            self.vertices.push(inds[1]);
        }
        if !self.vertices.contains(&inds[2]) {
            self.vertices.push(inds[2]);
        }
        self.recompute_border(triangle_neighbours);
        self.recompute_center(positions);

        true
    }

    fn recompute_border(&mut self, triangle_neighbours: &[Vec<Triangle>]) {
        self.border.clear();

        // Loop over every triangle in the meshlet and accumulate the set of triangles that are
        // neighbors to those triangles, but not contained within the meshlet itself.
        self.triangles.iter().for_each(|tri| {
            triangle_neighbours[tri.0 as usize]
                .iter()
                .for_each(|neighbour| {
                    if self.triangles.contains(neighbour) {
                        return;
                    }

                    if !self.border.contains(neighbour) {
                        self.border.push(*neighbour);
                    }
                });
        });
    }

    fn recompute_center(&mut self, positions: &[Vec4]) {
        // Sum the position of every vertex and average them.
        let mut sum = Vec3A::ZERO;
        self.vertices.iter().for_each(|v| {
            sum += Vec3A::from(positions[*v as usize].xyz());
        });
        self.center = sum / self.vertices.len() as f32;
    }
}

impl MeshClustifier {
    pub fn new(vertices: VertexData, indices: Vec<u32>) -> Self {
        assert_eq!(indices.len() % 3, 0);

        // Map each vertex to the triangles it belongs to
        let mut vertex_neighbours = vec![Vec::default(); vertices.len()];
        let mut unused_triangles = Vec::with_capacity(indices.len() / 3);
        indices.chunks_exact(3).enumerate().for_each(|(tri, idxs)| {
            let tri = Triangle::new(tri as u32);

            unused_triangles.push(tri);

            let v1 = &mut vertex_neighbours[idxs[0] as usize];
            if !v1.contains(&tri) {
                v1.push(tri);
            }

            let v2 = &mut vertex_neighbours[idxs[1] as usize];
            if !v2.contains(&tri) {
                v2.push(tri);
            }

            let v3 = &mut vertex_neighbours[idxs[2] as usize];
            if !v3.contains(&tri) {
                v3.push(tri);
            }
        });

        // Map each triangle to its neighbours
        let mut triangle_neighbours = vec![Vec::default(); indices.len() / 3];
        indices.chunks_exact(3).enumerate().for_each(|(tri, idxs)| {
            let tri_neighbours = &mut triangle_neighbours[tri];
            let tri = Triangle::new(tri as u32);

            idxs.iter().for_each(|idx| {
                vertex_neighbours[*idx as usize]
                    .iter()
                    .for_each(|neighbour| {
                        if *neighbour == tri {
                            return;
                        }

                        if !tri_neighbours.contains(neighbour) {
                            tri_neighbours.push(*neighbour);
                        }
                    });
            });
        });

        Self {
            unused_triangles,
            used_tri_lookup: vec![false; indices.len() / 3],
            vertices,
            indices,
            triangle_neighbours,
            meshlet: WorkingMeshlet::default(),
        }
    }

    pub fn build(mut self) -> MeshClustifierOutput {
        // Need at least one primitive
        if self.indices.len() == 0 || self.vertices.len() == 0 {
            return MeshClustifierOutput {
                vertices: self.vertices,
                indices: Vec::default(),
                meshlets: Vec::default(),
            };
        }

        let mut output_meshlets = Vec::default();

        // Add the first triangle to the working meshlet.
        self.meshlet.add(
            Triangle::new(0),
            &self.indices,
            self.vertices.positions(),
            &self.triangle_neighbours,
        );
        self.use_tri(Triangle::new(0));

        // Loop until we've used every triangle
        while !self.unused_triangles.is_empty() {
            // Find the best triangle from the border of the meshlet
            let mut best_tri = self.select_best_tri(
                self.meshlet.border.iter().map(|t| *t),
                &self.used_tri_lookup,
                false,
            );

            // If we couldn't find one, we need to look at triangles outside of the meshlet.
            if best_tri.is_none() {
                best_tri = self.select_best_tri(
                    self.unused_triangles.iter().map(|tri| *tri),
                    &self.used_tri_lookup,
                    false,
                );
            }

            // Since we still have unused triangles, we must have found one.
            let best_tri = best_tri.unwrap();

            // Add the triangle to the meshlet
            if self.meshlet.add(
                best_tri,
                &self.indices,
                self.vertices.positions(),
                &self.triangle_neighbours,
            ) {
                self.use_tri(best_tri);
                continue;
            }

            // If we failed to add the triangle, the meshlet must be full. However, it is possible
            // that we might have room for more primitives. We should try to fill up our primitive
            // count by checking to see if there are any primitives that contain vertices that are
            // all within the meshlet. These would be in the border of the meshlet.
            while self.meshlet.triangles.len() != Meshlet::MAX_PRIMITIVES {
                match self.select_best_tri(
                    self.meshlet.border.iter().map(|t| *t),
                    &self.used_tri_lookup,
                    true,
                ) {
                    // Add the triangle
                    Some(tri) => {
                        // If we fail to add it here, we are full on primitives.
                        if self.meshlet.add(
                            tri,
                            &self.indices,
                            self.vertices.positions(),
                            &self.triangle_neighbours,
                        ) {
                            self.use_tri(tri);
                        } else {
                            break;
                        }
                    }
                    // No more triangles to add.
                    None => break,
                }
            }

            // Now that our meshlet is full, we add it to the output list
            let mut meshlet = std::mem::take(&mut self.meshlet);
            meshlet.border = Vec::default();
            output_meshlets.push(meshlet);
        }

        // Check if we have a left over meshlet
        if !self.meshlet.is_empty() {
            let mut meshlet = std::mem::take(&mut self.meshlet);
            meshlet.border = Vec::default();
            output_meshlets.push(meshlet);
        }

        self.triangle_neighbours = Vec::default();
        self.unused_triangles = Vec::default();
        self.gen_output(output_meshlets)
    }

    /// Takes generated meshlets from `build` and produces the actual output of clusterization.
    fn gen_output(self, working_meshlets: Vec<WorkingMeshlet>) -> MeshClustifierOutput {
        let mut out = MeshClustifierOutput {
            vertices: VertexData::default(),
            indices: Vec::default(),
            meshlets: Vec::default(),
        };

        let mut idx_map = FxHashMap::default();
        working_meshlets.into_iter().for_each(|wm| {
            idx_map.clear();

            let mut meshlet = Meshlet {
                // Vertex offset is the current vertex length.
                vertex_offset: out.vertices.len() as u32,
                // Index offset is the current index length.
                index_offset: out.indices.len() as u32,
                // Will compute in later step.
                bounds: ObjectBounds::default(),
                vertex_count: wm.vertices.len() as u8,
                primitive_count: wm.triangles.len() as u8,
            };

            // Insert unique vertices into the new buffer, and creating a mapping from the old
            // index to the new index.
            wm.vertices.iter().for_each(|idx| {
                idx_map.insert(
                    *idx,
                    (out.vertices.len() as u32 - meshlet.vertex_offset) as u8,
                );
                out.vertices.append_from(&self.vertices, *idx);
            });

            // Append output indices from the working meshlet primitives
            wm.triangles.iter().for_each(|tri| {
                let verts = self.verts(*tri);
                out.indices.push(*idx_map.get(&verts[0]).unwrap());
                out.indices.push(*idx_map.get(&verts[1]).unwrap());
                out.indices.push(*idx_map.get(&verts[2]).unwrap());
            });

            // Compute object bounds
            meshlet.bounds = ObjectBounds::from_positions(
                &out.vertices.positions()[meshlet.vertex_offset as usize
                    ..(meshlet.vertex_offset as usize + meshlet.vertex_count as usize)],
            );

            out.meshlets.push(meshlet);
        });

        out.vertices.compute_bounds();

        out
    }

    /// Selects the best triangle from a given iterator for the current meshlet. Returns `None`
    /// if the iterator is empty or it contains only triangles that are in the meshlet.
    fn select_best_tri(
        &self,
        tris: impl Iterator<Item = Triangle>,
        used: &Vec<bool>,
        must_share_all_verts: bool,
    ) -> Option<Triangle> {
        let mut best_tri = None;
        let mut best_radius = f32::MAX;
        let mut best_shared_verts = 0;

        tris.for_each(|tri| {
            // Skip if this triangle is already in the meshlet.
            if self.meshlet.triangles.contains(&tri) {
                return;
            }

            // Skip if this triangle is already used.
            if used[tri.0 as usize] {
                return;
            }

            // Count how many vertices this triangle shares with the meshlet
            let verts = self.verts(tri);
            let shared_verts: u32 = verts
                .iter()
                .map(|vert| self.meshlet.vertices.contains(vert) as u32)
                .sum();

            // If we must have all shared vertices and this one doesn't, skip it
            if must_share_all_verts && shared_verts != 3 {
                return;
            }

            // Determine how far away the triangle is from the center of the meshlet.
            let mut dist: f32 = 0.0;
            verts.iter().for_each(|v| {
                dist = dist.max(
                    (self.meshlet.center
                        - Vec3A::from(self.vertices.positions()[*v as usize].xyz()))
                    .length(),
                );
            });

            // We prefer triangles that share more vertices over triangles that reduce the bounding
            // sphere radius.
            if shared_verts > best_shared_verts
                || (shared_verts == best_shared_verts && dist <= best_radius)
            {
                best_tri = Some(tri);
                best_radius = dist;
                best_shared_verts = shared_verts;
            }
        });

        best_tri
    }

    #[inline(always)]
    fn verts(&self, tri: Triangle) -> [u32; 3] {
        let base = tri.0 as usize * 3;
        [
            self.indices[base],
            self.indices[base + 1],
            self.indices[base + 2],
        ]
    }

    #[inline(always)]
    fn use_tri(&mut self, tri: Triangle) {
        let mut idx = usize::MAX;
        for (i, found_tri) in self.unused_triangles.iter().enumerate() {
            if *found_tri == tri {
                idx = i;
                break;
            }
        }
        self.used_tri_lookup[tri.0 as usize] = true;
        self.unused_triangles.swap_remove(idx);
    }
}
