use std::ops::{DerefMut, Range};

use ard_formats::mesh::VertexLayout;
use ard_log::*;
use ard_math::*;
use ard_pal::prelude::*;
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{
    factory::{MaterialFactory, PassId},
    material::{MaterialResource, MaterialVariantRequest},
};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::set::DrawGroup;
use ard_render_si::types::{GpuDrawCall, GpuDrawGroup};
use ard_render_textures::factory::TextureFactory;

use crate::{
    calls::OutputDrawCalls,
    state::{BindingDelta, RenderStateTracker},
};

pub const DEFAULT_DRAW_GROUP_CAP: usize = 1;
pub const DEFAULT_DRAW_COUNT_CAP: usize = 1;

pub struct DrawBins {
    bins: Vec<DrawBinSet>,
    use_alternate_draw_buffer: Vec<bool>,
    src_draw_groups: Buffer,
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
    pub draw_count: usize,
    /// The number of objects processed.
    pub object_count: usize,
    /// The number of bins generated.
    pub bin_count: usize,
}

pub struct RenderArgs<'a, 'b, const FIF: usize> {
    pub pass_id: PassId,
    pub frame: Frame,
    pub pass: &'b mut RenderPass<'a>,
    pub camera: &'a CameraUbo,
    pub global_set: &'a DescriptorSet,
    pub calls: &'a OutputDrawCalls,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory<FIF>,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource, FIF>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
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
    /// Contains the resource ID for the material instance to use, or `None` if the previous bin's
    /// material was identical.
    pub material: Option<ResourceId>,
    /// Contains the vertex layout to use, or `None` if the previous bin's layout was identical.
    pub vertices: Option<VertexLayout>,
    /// Contains the material data size needed for this bin, or `None` if the previous bin's
    /// data size was identical.
    pub data_size: Option<u32>,
}

impl DrawBins {
    pub fn new(ctx: &Context, frames_in_flight: usize) -> Self {
        Self {
            bins: (0..(frames_in_flight * 2))
                .map(|_| DrawBinSet::default())
                .collect(),
            src_draw_groups: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_DRAW_GROUP_CAP * std::mem::size_of::<GpuDrawGroup>()) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("src_draw_groups".to_owned()),
                },
            )
            .unwrap(),
            use_alternate_draw_buffer: vec![false; frames_in_flight],
        }
    }

    #[inline(always)]
    pub fn has_valid_draws(&self, frame: Frame) -> bool {
        let draw_call_idx = self.draw_call_buffer_idx(frame);
        self.bins[draw_call_idx].has_valid_draws
    }

    #[inline(always)]
    pub fn use_alternate(&self, frame: Frame) -> bool {
        self.use_alternate_draw_buffer[usize::from(frame)]
    }

    #[inline(always)]
    pub fn bins(&self, frame: Frame) -> &[DrawBin] {
        let draw_call_idx = self.draw_call_buffer_idx(frame);
        &self.bins[draw_call_idx].bins
    }

    #[inline(always)]
    pub fn draw_groups_buffer(&self, frame: Frame) -> (&Buffer, usize) {
        (&self.src_draw_groups, usize::from(frame))
    }

    pub fn preallocate_draw_group_buffers(&mut self, draw_call_count: usize) {
        let src_req_size = (draw_call_count * std::mem::size_of::<GpuDrawGroup>()) as u64;
        if let Some(new_buff) = Buffer::expand(&self.src_draw_groups, src_req_size, true) {
            self.src_draw_groups = new_buff;
        }
    }

    /// Generates high-z culling bins.
    pub fn gen_bins<'a, 'b, const FIF: usize>(
        &'b mut self,
        frame: Frame,
        static_opaque_draws: impl Iterator<Item = &'a DrawGroup>,
        static_ac_draws: impl Iterator<Item = &'a DrawGroup>,
        dynamic_opaque_draws: impl Iterator<Item = &'a DrawGroup>,
        dynamic_ac_draws: impl Iterator<Item = &'a DrawGroup>,
        transparent_draws: impl Iterator<Item = &'a DrawGroup>,
        meshes: &'a ResourceAllocator<MeshResource, FIF>,
        materials: &'a ResourceAllocator<MaterialResource, FIF>,
    ) {
        // Switch to alternate buffer
        self.use_alternate_draw_buffer[usize::from(frame)] =
            !self.use_alternate_draw_buffer[usize::from(frame)];
        let draw_call_idx = self.draw_call_buffer_idx(frame);

        // Pull the view for the buffer
        let mut draw_call_view = self.src_draw_groups.write(usize::from(frame)).unwrap();
        let draw_group_slice =
            bytemuck::cast_slice_mut::<_, GpuDrawGroup>(draw_call_view.deref_mut());

        // Grab bin set and reset
        let bin_set = &mut self.bins[draw_call_idx];
        bin_set.bins.clear();
        bin_set.has_valid_draws = false;

        let mut call_offset = 0;
        let mut id_offset = 0;

        // Static opaque
        bin_set.static_opaque.start = 0;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            static_opaque_draws.zip(draw_group_slice.iter_mut()),
            meshes,
            materials,
            call_offset,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        call_offset += res.draw_count;
        id_offset += res.object_count;
        bin_set.static_opaque.end = res.bin_count;

        // Static alpha cutoff
        bin_set.static_ac.start = bin_set.static_opaque.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            static_ac_draws.zip(draw_group_slice.iter_mut().skip(call_offset)),
            meshes,
            materials,
            call_offset,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        call_offset += res.draw_count;
        id_offset += res.object_count;
        bin_set.static_ac.end = bin_set.static_ac.start + res.bin_count;

        // Dynamic opaque
        bin_set.dynamic_opaque.start = bin_set.static_ac.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            dynamic_opaque_draws.zip(draw_group_slice.iter_mut().skip(call_offset)),
            meshes,
            materials,
            call_offset,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        call_offset += res.draw_count;
        id_offset += res.object_count;
        bin_set.dynamic_opaque.end = bin_set.dynamic_opaque.start + res.bin_count;

        // Dynamic alpha cutoff
        bin_set.dynamic_ac.start = bin_set.dynamic_opaque.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            dynamic_ac_draws.zip(draw_group_slice.iter_mut().skip(call_offset)),
            meshes,
            materials,
            call_offset,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        call_offset += res.draw_count;
        id_offset += res.object_count;
        bin_set.dynamic_ac.end = bin_set.dynamic_ac.start + res.bin_count;

        // Transparent
        bin_set.transparent_rng.start = bin_set.dynamic_ac.end;
        let res = Self::gen_bins_inner(
            &mut bin_set.bins,
            transparent_draws.zip(draw_group_slice.iter_mut().skip(call_offset)),
            meshes,
            materials,
            call_offset,
            id_offset,
            &mut bin_set.has_valid_draws,
        );
        bin_set.transparent_rng.end = bin_set.transparent_rng.start + res.bin_count;
    }

    /// Appends bins from the provided grouped draws.
    fn gen_bins_inner<'a, 'b, const FIF: usize>(
        bins: &mut Vec<DrawBin>,
        grouped_draws: impl Iterator<Item = (&'a DrawGroup, &'b mut GpuDrawGroup)>,
        meshes: &ResourceAllocator<MeshResource, FIF>,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        mut draw_call_offset: usize,
        mut object_id_offset: usize,
        has_valid_draws: &mut bool,
    ) -> BinGenOutput {
        let mut out = BinGenOutput {
            draw_count: 0,
            object_count: 0,
            bin_count: 0,
        };

        // State tracking to generate bins
        let mut state = RenderStateTracker::default();
        let mut draw_count: usize = 0;
        let mut delta = BindingDelta::default();
        let mut first = true;

        // Generate bins for all groups
        for (group, draw_call) in grouped_draws {
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
                    count: draw_count,
                    offset: draw_call_offset,
                    skip: delta.skip,
                    material: delta.new_material,
                    vertices: delta.new_vertices,
                    data_size: delta.new_data_size,
                });

                if !delta.skip {
                    *has_valid_draws = true;
                }

                draw_call_offset += draw_count;
                out.bin_count += 1;
                draw_count = 0;

                delta = new_delta;
            }

            // Write the draw call for this group
            let first_instance = object_id_offset as u32;
            object_id_offset += group.len;
            out.object_count += group.len;

            *draw_call = match meshes.get(separated_key.mesh_id) {
                Some(_) => GpuDrawGroup {
                    mesh: usize::from(separated_key.mesh_id) as u32,
                    instance_count: group.len as u32,
                    first_instance,
                    draw_bin: bins.len() as u32,
                    bin_offset: draw_count as u32,
                },
                None => {
                    warn!(
                        "Attempted to render mesh with ID `{:?}` that did not exist. Skipping draw.",
                        separated_key.mesh_id,
                    );
                    GpuDrawGroup {
                        mesh: 0,
                        instance_count: 0,
                        first_instance: 0,
                        draw_bin: 0,
                        bin_offset: 0,
                    }
                }
            };

            draw_count += 1;
            out.draw_count += 1;
        }

        // Add in the final draw if it wasn't registered
        if draw_count > 0 {
            bins.push(DrawBin {
                count: draw_count,
                offset: draw_call_offset,
                skip: delta.skip,
                material: delta.new_material,
                vertices: delta.new_vertices,
                data_size: delta.new_data_size,
            });
            out.bin_count += 1;
        }

        out
    }

    #[inline(always)]
    fn draw_call_buffer_idx(&self, frame: Frame) -> usize {
        let frame = usize::from(frame);
        (frame * 2) + self.use_alternate_draw_buffer[frame] as usize
    }

    pub fn render_static_opaque_bins<'a, const FIF: usize>(&'a self, use_last: bool, args: RenderArgs<'a, '_, FIF>) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let use_alternate = self.use_alternate_draw_buffer[usize::from(args.frame)];
        let set = &self.bins[idx];

        let (draw_calls, draw_counts) = if use_last {
            (
                args.calls.last_draw_call_buffer(args.frame, use_alternate),
                args.calls.last_draw_count_buffer(args.frame, use_alternate)
            )
        } else {
            (
                args.calls.draw_call_buffer(args.frame, use_alternate),
                args.calls.draw_counts_buffer(args.frame, use_alternate)
            )
        };

        self.render_bins(
            args,
            draw_calls,
            draw_counts,
            set.static_opaque.start,
            set.static_opaque.len(),
        );
    }

    pub fn render_static_alpha_cutoff_bins<'a, const FIF: usize>(&'a self, args: RenderArgs<'a, '_, FIF>) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let use_alternate = self.use_alternate_draw_buffer[usize::from(args.frame)];
        let set = &self.bins[idx];
        let draw_calls = args.calls.draw_call_buffer(args.frame, use_alternate);
        let draw_counts = args.calls.draw_counts_buffer(args.frame, use_alternate);
        self.render_bins(
            args,
            draw_calls,
            draw_counts,
            set.static_ac.start,
            set.static_ac.len(),
        );
    }

    pub fn render_dynamic_opaque_bins<'a, const FIF: usize>(&'a self, args: RenderArgs<'a, '_, FIF>) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let use_alternate = self.use_alternate_draw_buffer[usize::from(args.frame)];
        let set = &self.bins[idx];
        let draw_calls = args.calls.draw_call_buffer(args.frame, use_alternate);
        let draw_counts = args.calls.draw_counts_buffer(args.frame, use_alternate);
        self.render_bins(
            args,
            draw_calls,
            draw_counts,
            set.dynamic_opaque.start,
            set.dynamic_opaque.len(),
        );
    }

    pub fn render_dynamic_alpha_cutoff_bins<'a, const FIF: usize>(&'a self, args: RenderArgs<'a, '_, FIF>) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let use_alternate = self.use_alternate_draw_buffer[usize::from(args.frame)];
        let set = &self.bins[idx];
        let draw_calls = args.calls.last_draw_call_buffer(args.frame, use_alternate);
        let draw_counts = args.calls.draw_counts_buffer(args.frame, use_alternate);
        self.render_bins(
            args,
            draw_calls,
            draw_counts,
            set.dynamic_ac.start,
            set.dynamic_ac.len(),
        );
    }

    pub fn render_transparent_bins<'a, const FIF: usize>(&'a self, args: RenderArgs<'a, '_, FIF>) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let use_alternate = self.use_alternate_draw_buffer[usize::from(args.frame)];
        let set = &self.bins[idx];
        let draw_calls = args.calls.draw_call_buffer(args.frame, use_alternate);
        let draw_counts = args.calls.draw_counts_buffer(args.frame, use_alternate);
        self.render_bins(
            args,
            draw_calls,
            draw_counts,
            set.transparent_rng.start,
            set.transparent_rng.len(),
        );
    }

    fn render_bins<'a, const FIF: usize>(
        &'a self,
        mut args: RenderArgs<'a, '_, FIF>,
        draw_calls: (&'a Buffer, usize),
        draw_counts: (&'a Buffer, usize),
        bin_offset: usize,
        bin_count: usize,
    ) {
        let idx = self.draw_call_buffer_idx(args.frame);
        let mut mat_vertex_layout = VertexLayout::empty();
        let mut mesh_vertex_layout = VertexLayout::empty();
        let mut mat_id = ResourceId::from(usize::MAX);
        let mut variant_id = u32::MAX;
        let mut has_bound_global = false;

        for (bin_idx, bin) in self.bins[idx]
            .bins
            .iter()
            .enumerate()
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
                mat_vertex_layout = variant.vertex_layout;

                // Bind variant pipeline
                args.pass.bind_pipeline(variant.pipeline.clone());
                rebound_material = true;

                // Bind global sets
                if !has_bound_global {
                    args.bind_global();
                    has_bound_global = true;
                }
            }

            if let Some(data_size) = bin.data_size {
                if data_size > 0 {
                    // Bind material set
                    args.pass.bind_sets(
                        3,
                        vec![args
                            .material_factory
                            .get_set(args.frame, data_size as u64)
                            .unwrap()],
                    );
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
                        mat_vertex_layout = variant.vertex_layout;

                        // Bind variant pipeline
                        args.pass.bind_pipeline(variant.pipeline.clone());

                        // Bind global sets
                        if !has_bound_global {
                            args.bind_global();
                            has_bound_global = true;
                        }
                    }
                }

                // NOTE: Vertex buffer type must exist if we have a valid mesh that uses it's layout
                mesh_vertex_layout = vertex_layout;
                let vbuffer = args.mesh_factory.vertex_buffer(vertex_layout).unwrap();
                vbuffer.bind(args.pass, mat_vertex_layout).unwrap();
            }

            // Skip if requested
            if bin.skip {
                continue;
            }

            // Perform the draw call
            args.pass.draw_indexed_indirect_count(
                draw_calls.0,
                draw_calls.1,
                (bin.offset * std::mem::size_of::<GpuDrawCall>()) as u64,
                draw_counts.0,
                draw_counts.1,
                (bin_idx * 2 * std::mem::size_of::<u32>()) as u64,
                bin.count,
                std::mem::size_of::<GpuDrawCall>() as u64,
            );
        }
    }
}

impl<'a, 'b, const FIF: usize> RenderArgs<'a, 'b, FIF> {
    fn bind_global(&mut self) {
        self.pass
            .bind_sets(0, vec![self.global_set, self.camera.get_set(self.frame)]);

        // SAFETY: This is safe as long as:
        // 1. We transition to SAMPLED usage in the factory during texture mip upload.
        // 2. We don't write to the texture after it's been uploaded.
        unsafe {
            self.pass
                .bind_sets_unchecked(2, vec![self.texture_factory.get_set(self.frame)]);
        }
    }
}
