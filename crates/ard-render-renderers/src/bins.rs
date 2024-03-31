use std::ops::Range;

use ard_formats::vertex::VertexLayout;
use ard_math::*;
use ard_pal::prelude::*;
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
    FRAMES_IN_FLIGHT,
};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{
    factory::{MaterialFactory, PassId},
    material::{MaterialResource, MaterialVariantRequest},
};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::set::DrawGroup;
use ard_render_si::types::*;
use ard_render_textures::factory::TextureFactory;

use crate::state::{BindingDelta, RenderStateTracker};

pub struct DrawBins {
    bins: [DrawBinSet; FRAMES_IN_FLIGHT],
}

#[derive(Default)]
struct DrawBinSet {
    bins: Vec<DrawBin>,
    has_valid_draws: bool,
    static_opaque: Range<usize>,
    static_ac: Range<usize>,
    dynamic_opaque: Range<usize>,
    dynamic_ac: Range<usize>,
    transparent_rng: Range<usize>,
}

#[derive(Debug, Copy, Clone)]
pub struct BinGenOutput {
    /// The number of draw calls proccessed.
    // pub draw_count: usize,
    /// The number of objects processed.
    pub object_count: usize,
    /// The number of bins generated.
    pub bin_count: usize,
}

pub struct RenderArgs<'a, 'b> {
    pub ctx: &'a Context,
    pub pass_id: PassId,
    pub frame: Frame,
    pub lock_culling: bool,
    pub render_area: Vec2,
    pub pass: &'b mut RenderPass<'a>,
    pub camera: &'a CameraUbo,
    pub global_set: &'a DescriptorSet,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource>,
    pub materials: &'a ResourceAllocator<MaterialResource>,
}

/// A draw bin represents a set of draw groups that have the same vertex layout and material,
/// meaning they can all be rendered with a single draw indirect command.
#[derive(Debug, Copy, Clone)]
pub struct DrawBin {
    /// Indicates that this draw bin should be skipped when rendering. This is usually because some
    /// resource (like the mesh) is not ready for rendering.
    pub skip: bool,
    /// The number of draw calls contained within the bin.
    pub count: usize,
    /// Offset (measured in draw calls) for the beginning of this bin.
    pub offset: usize,
    /// Contains the resource ID for the material to use, or `None` if the previous bin's
    /// material was identical.
    pub material: Option<ResourceId>,
    /// Contains the vertex layout to use, or `None` if the previous bin's layout was identical.
    pub vertices: Option<VertexLayout>,
    /// Contains the material data size needed for this bin, or `None` if the previous bin's
    /// data size was identical.
    pub data_size: Option<u32>,
}

impl DrawBins {
    pub fn new() -> Self {
        Self {
            bins: std::array::from_fn(|_| DrawBinSet::default()),
        }
    }

    #[inline(always)]
    pub fn has_valid_draws(&self, frame: Frame) -> bool {
        self.bins[usize::from(frame)].has_valid_draws
    }

    #[inline(always)]
    pub fn bins(&self, frame: Frame) -> &[DrawBin] {
        &self.bins[usize::from(frame)].bins
    }

    /// Generates high-z culling bins.
    pub fn gen_bins<'a, 'b>(
        &'b mut self,
        frame: Frame,
        static_opaque_draws: impl Iterator<Item = &'a DrawGroup>,
        static_ac_draws: impl Iterator<Item = &'a DrawGroup>,
        dynamic_opaque_draws: impl Iterator<Item = &'a DrawGroup>,
        dynamic_ac_draws: impl Iterator<Item = &'a DrawGroup>,
        transparent_draws: impl Iterator<Item = &'a DrawGroup>,
        meshes: &'a ResourceAllocator<MeshResource>,
        materials: &'a ResourceAllocator<MaterialResource>,
    ) {
        let draw_call_idx = usize::from(frame);

        // Grab bin set and reset
        let bin_set = &mut self.bins[draw_call_idx];
        bin_set.bins.clear();
        bin_set.has_valid_draws = false;

        let mut id_offset = 0;

        // Static opaque
        bin_set.static_opaque.start = 0;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            static_opaque_draws,
            meshes,
            materials,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        id_offset += res.object_count;
        bin_set.static_opaque.end = res.bin_count;

        // Static alpha cutoff
        bin_set.static_ac.start = bin_set.static_opaque.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            static_ac_draws,
            meshes,
            materials,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        id_offset += res.object_count;
        bin_set.static_ac.end = bin_set.static_ac.start + res.bin_count;

        // Dynamic opaque
        bin_set.dynamic_opaque.start = bin_set.static_ac.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            dynamic_opaque_draws,
            meshes,
            materials,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        id_offset += res.object_count;
        bin_set.dynamic_opaque.end = bin_set.dynamic_opaque.start + res.bin_count;

        // Dynamic alpha cutoff
        bin_set.dynamic_ac.start = bin_set.dynamic_opaque.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            dynamic_ac_draws,
            meshes,
            materials,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        id_offset += res.object_count;
        bin_set.dynamic_ac.end = bin_set.dynamic_ac.start + res.bin_count;

        // Transparent
        bin_set.transparent_rng.start = bin_set.dynamic_ac.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            transparent_draws,
            meshes,
            materials,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        bin_set.transparent_rng.end = bin_set.transparent_rng.start + res.bin_count;
    }

    /// Appends bins from the provided grouped draws.
    fn gen_bins_inner<'a>(
        bins: &mut Vec<DrawBin>,
        groups: impl Iterator<Item = &'a DrawGroup>,
        meshes: &ResourceAllocator<MeshResource>,
        materials: &ResourceAllocator<MaterialResource>,
        mut object_id_offset: usize,
        has_valid_draws: &mut bool,
    ) -> BinGenOutput {
        let mut out = BinGenOutput {
            object_count: 0,
            bin_count: 0,
        };

        // State tracking to generate bins
        let mut state = RenderStateTracker::default();
        let mut draw_count: usize = 0;
        let mut delta = BindingDelta::default();
        let mut first = true;

        let mut object_offset = object_id_offset;
        let mut bin_object_count = 0;

        // Generate bins for all groups
        for group in groups {
            let separated_key = group.key.separate();

            // If this is the first group, we must prime the delta
            // TODO: Get this check out of the hot loop
            if first {
                delta = state.compute_delta(&separated_key, meshes, materials);
                first = false;
            }

            // Compute the delta for this draw and see if it's new
            let new_delta = state.compute_delta(&separated_key, meshes, materials);

            // If it is, we submit the current group
            if new_delta.draw_required() {
                bins.push(DrawBin {
                    count: bin_object_count,
                    offset: object_offset,
                    skip: delta.skip,
                    material: delta.new_material,
                    vertices: delta.new_vertices,
                    data_size: delta.new_data_size,
                });

                if !delta.skip {
                    *has_valid_draws = true;
                }

                out.bin_count += 1;
                draw_count = 0;
                object_offset += bin_object_count;
                bin_object_count = 0;

                delta = new_delta;
            }

            // Write the draw call for this group
            object_id_offset += group.len;
            out.object_count += group.len;
            bin_object_count += group.len;
            draw_count += 1;
        }

        // Add in the final draw if it wasn't registered
        if draw_count > 0 {
            bins.push(DrawBin {
                count: bin_object_count,
                offset: object_offset,
                skip: delta.skip,
                material: delta.new_material,
                vertices: delta.new_vertices,
                data_size: delta.new_data_size,
            });
            out.bin_count += 1;
        }

        out
    }

    pub fn render_static_opaque_bins<'a>(&'a self, args: RenderArgs<'a, '_>) {
        let set = &self.bins[usize::from(args.frame)];
        self.render_bins(args, set.static_opaque.start, set.static_opaque.len());
    }

    pub fn render_static_alpha_cutoff_bins<'a>(&'a self, args: RenderArgs<'a, '_>) {
        let set = &self.bins[usize::from(args.frame)];
        self.render_bins(args, set.static_ac.start, set.static_ac.len());
    }

    pub fn render_dynamic_opaque_bins<'a>(&'a self, args: RenderArgs<'a, '_>) {
        let set = &self.bins[usize::from(args.frame)];
        self.render_bins(args, set.dynamic_opaque.start, set.dynamic_opaque.len());
    }

    pub fn render_dynamic_alpha_cutoff_bins<'a>(&'a self, args: RenderArgs<'a, '_>) {
        let set = &self.bins[usize::from(args.frame)];
        self.render_bins(args, set.dynamic_ac.start, set.dynamic_ac.len());
    }

    pub fn render_transparent_bins<'a>(&'a self, args: RenderArgs<'a, '_>) {
        let set = &self.bins[usize::from(args.frame)];
        self.render_bins(args, set.transparent_rng.start, set.transparent_rng.len());
    }

    fn render_bins<'a>(
        &'a self,
        mut args: RenderArgs<'a, '_>,
        bin_offset: usize,
        bin_count: usize,
    ) {
        let mut mesh_vertex_layout = VertexLayout::empty();
        let mut mat_id = ResourceId::from(usize::MAX);
        let mut variant_id = u32::MAX;
        let mut has_bound_global = false;

        for bin in self.bins[usize::from(args.frame)]
            .bins
            .iter()
            .skip(bin_offset)
            .take(bin_count)
        {
            // Pre-check for meshes vertex layout because it's needed by the material
            if let Some(vertex_layout) = bin.vertices {
                mesh_vertex_layout = vertex_layout;
            }

            // Perform rebindings
            let mut rebound_material = false;
            if let Some(new_mat_id) = bin.material {
                mat_id = new_mat_id;

                let mat = args.materials.get(mat_id).unwrap();
                let variant = match mat.get_variant(MaterialVariantRequest {
                    pass_id: args.pass_id,
                    vertex_layout: mesh_vertex_layout,
                }) {
                    Some(variant) => variant,
                    None => panic!(
                        "Attempt to render with material `{:?}` with vertex layout \
                    `{:?}` but there were no supported variants.",
                        mat_id, mesh_vertex_layout
                    ),
                };

                variant_id = variant.id;

                // Bind variant pipeline
                args.pass.bind_pipeline(variant.pipeline.clone());
                rebound_material = true;

                // Bind global sets
                if !has_bound_global {
                    args.bind_global();
                    has_bound_global = true;
                }
            }

            if let Some(vertex_layout) = bin.vertices {
                // If the vertices have changed, we also must check if the shader variant has
                // changed, but only if the pipeline was not rebound
                if !rebound_material {
                    let mat = args.materials.get(mat_id).unwrap();
                    let variant = match mat.get_variant(MaterialVariantRequest {
                        pass_id: args.pass_id,
                        vertex_layout: mesh_vertex_layout,
                    }) {
                        Some(variant) => variant,
                        None => panic!(
                            "Attempt to render with material `{:?}` with vertex layout \
                        `{:?}` but there were no supported variants.",
                            mat_id, mesh_vertex_layout
                        ),
                    };

                    if variant.id != variant_id {
                        variant_id = variant.id;

                        // Bind variant pipeline
                        args.pass.bind_pipeline(variant.pipeline.clone());

                        // Bind global sets
                        if !has_bound_global {
                            args.bind_global();
                            has_bound_global = true;
                        }
                    }
                }

                mesh_vertex_layout = vertex_layout;
            }

            // Skip if requested
            if bin.skip {
                continue;
            }

            // Object offset and count for the bin.
            let constants = [GpuDrawPushConstants {
                object_id_offset: bin.offset as u32,
                object_id_count: bin.count as u32,
                render_area: args.render_area,
                lock_culling: args.lock_culling as u32,
            }];
            args.pass.push_constants(bytemuck::cast_slice(&constants));
            args.pass.draw_mesh_tasks(bin.count as u32, 1, 1);
        }
    }
}

impl<'a, 'b> RenderArgs<'a, 'b> {
    fn bind_global(&mut self) {
        self.pass.bind_sets(
            0,
            vec![
                self.global_set,
                self.camera.get_set(self.frame),
                self.mesh_factory.mesh_data_set(self.frame),
                self.material_factory.get_texture_slots_set(self.frame),
            ],
        );

        // SAFETY: This is safe as long as:
        // 1. We transition to SAMPLED usage in the factory during texture mip upload.
        // 2. We don't write to the texture after it's been uploaded.
        unsafe {
            self.pass
                .bind_sets_unchecked(4, vec![self.texture_factory.get_set(self.frame)]);
        }
    }
}
