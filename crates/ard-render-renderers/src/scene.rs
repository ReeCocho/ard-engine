use std::ops::DerefMut;

use ard_ecs::prelude::*;
use ard_formats::mesh::IndexData;
use ard_log::warn;
use ard_math::{Vec3A, Vec4};
use ard_pal::prelude::{Buffer, BufferCreateInfo, BufferUsage, Context, MemoryUsage, RenderPass};
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::material::MaterialResource;
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
};
use ard_render_si::{
    bindings::Layouts,
    types::{GpuDrawCall, GpuObjectBounds, GpuObjectId},
};

use crate::{
    draw_gen::{DrawGenPipeline, DrawGenSets},
    global::GlobalSets,
    state::{RenderArgs, RenderStateTracker},
    DEPTH_PREPASS_PASS_ID, HIGH_Z_PASS_ID, OPAQUE_PASS_ID,
};

pub const DEFAULT_INPUT_ID_CAP: usize = 1;
pub const DEFAULT_OUTPUT_ID_CAP: usize = 1;
pub const DEFAULT_DRAW_CALL_CAP: usize = 1;

/// Primary GPU driven scene renderer.
#[derive(Resource)]
pub struct SceneRenderer {
    /// Object IDs which are filtered using the GPU driven frustum and occlusion culling compute
    /// shaders.
    input_ids: Buffer,
    /// IDs output from the culling computer shader to be bound when actual rendering is performed.
    output_ids: Buffer,
    /// Indirect draw call buffer.
    draw_calls: Buffer,
    /// Object information.
    set: RenderableSet,
    /// Set bindings for draw generation.
    draw_gen: DrawGenSets,
    /// Global bindings for rendering.
    global: GlobalSets,
    /// Indicates we should use the "alternate" draw call buffer for each frame in flight.
    ///
    /// This is needed because of how occlusion culling works. To generate the depth buffer used
    /// for high-z culling, we have to render last frame's static objects. Since we also need to
    /// reset draw counts on the CPU, we can't just use the same buffer (since we'd overwrite the
    /// draw counts we care about). So, for each frame in flight, we alternate between two draw
    /// call buffers.
    use_alternate: Vec<bool>,
}

pub struct SceneRenderArgs<'a, 'b, const FIF: usize> {
    pub pass: &'b mut RenderPass<'a>,
    pub camera: &'a CameraUbo,
    pub mesh_factory: &'a MeshFactory,
    pub meshes: &'a ResourceAllocator<MeshResource, FIF>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
}

impl SceneRenderer {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        draw_gen: &DrawGenPipeline,
        frames_in_flight: usize,
    ) -> Self {
        Self {
            input_ids: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_INPUT_ID_CAP * std::mem::size_of::<GpuObjectId>()) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    debug_name: Some("input_ids".to_owned()),
                },
            )
            .unwrap(),
            output_ids: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_OUTPUT_ID_CAP * std::mem::size_of::<u32>()) as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some("output_ids".to_owned()),
                },
            )
            .unwrap(),
            draw_calls: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<GpuDrawCall>()) as u64,
                    array_elements: frames_in_flight * 2,
                    buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    debug_name: Some("draw_calls".to_owned()),
                },
            )
            .unwrap(),
            set: RenderableSet::default(),
            global: GlobalSets::new(ctx, layouts, frames_in_flight),
            draw_gen: DrawGenSets::new(draw_gen, frames_in_flight),
            use_alternate: (0..frames_in_flight).map(|_| false).collect(),
        }
    }

    #[inline(always)]
    pub fn draw_gen_sets(&self) -> &DrawGenSets {
        &self.draw_gen
    }

    #[inline(always)]
    pub fn draw_call_buffer_idx(&self, frame: Frame) -> usize {
        let frame = usize::from(frame);
        (frame * 2) + self.use_alternate[frame] as usize
    }

    #[inline(always)]
    pub fn last_draw_call_buffer_idx(&self, frame: Frame) -> usize {
        let frame = usize::from(frame);
        (frame * 2) + !self.use_alternate[frame] as usize
    }

    pub fn upload<const FIF: usize>(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource, FIF>,
        view_location: Vec3A,
    ) {
        // Update the set with all objects to render
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .with_transparent()
            .update(frame, view_location, objects, |_| true, |_| true, |_| true);

        // Expand ID buffers if needed
        let input_id_buffer_size =
            (self.set.ids().len() * std::mem::size_of::<GpuObjectId>()) as u64;
        let input_id_buffer_expanded =
            match Buffer::expand(&self.input_ids, input_id_buffer_size, false) {
                Some(mut new_buffer) => {
                    std::mem::swap(&mut self.input_ids, &mut new_buffer);
                    true
                }
                None => false,
            };

        let output_id_buffer_size = (self.set.ids().len() * std::mem::size_of::<u32>()) as u64;
        if let Some(mut new_buffer) = Buffer::expand(&self.output_ids, output_id_buffer_size, false)
        {
            std::mem::swap(&mut self.output_ids, &mut new_buffer);
        }

        // Write in object IDs
        let mut id_view = self.input_ids.write(usize::from(frame)).unwrap();
        let id_slice = bytemuck::cast_slice_mut::<_, GpuObjectId>(id_view.deref_mut());

        // Write in static ids if they were modified
        if input_id_buffer_expanded || objects.static_dirty(frame) {
            id_slice[self.set.static_object_ranges().opaque.clone()]
                .copy_from_slice(&self.set.ids()[self.set.static_object_ranges().opaque.clone()]);
            id_slice[self.set.static_object_ranges().alpha_cutout.clone()].copy_from_slice(
                &self.set.ids()[self.set.static_object_ranges().alpha_cutout.clone()],
            );
        }

        // Write in dynamic object IDs
        id_slice[self.set.dynamic_object_ranges().opaque.clone()]
            .copy_from_slice(&self.set.ids()[self.set.dynamic_object_ranges().opaque.clone()]);
        id_slice[self.set.dynamic_object_ranges().alpha_cutout.clone()].copy_from_slice(
            &self.set.ids()[self.set.dynamic_object_ranges().alpha_cutout.clone()],
        );

        // Write in transparent object IDs
        id_slice[self.set.dynamic_object_ranges().transparent.clone()]
            .copy_from_slice(&self.set.ids()[self.set.dynamic_object_ranges().transparent.clone()]);
        id_slice[self.set.static_object_ranges().transparent.clone()]
            .copy_from_slice(&self.set.ids()[self.set.static_object_ranges().transparent.clone()]);

        // Expand draw call buffer if needed
        let draw_call_buffer_size =
            (self.set.groups().len() * std::mem::size_of::<GpuDrawCall>()) as u64;
        if let Some(buffer) = Buffer::expand(&self.draw_calls, draw_call_buffer_size, true) {
            self.draw_calls = buffer;
        };

        // Write in the draw calls
        self.use_alternate[usize::from(frame)] = !self.use_alternate[usize::from(frame)];
        let buffer_idx = self.draw_call_buffer_idx(frame);
        let mut draw_call_view = self.draw_calls.write(buffer_idx).unwrap();
        let draw_call_slice =
            bytemuck::cast_slice_mut::<_, GpuDrawCall>(draw_call_view.deref_mut());

        let mut object_offset = 0;

        for (group, draw_call) in self.set.groups().iter().zip(draw_call_slice.iter_mut()) {
            let separated_key = group.key.separate();
            let first_instance = object_offset as u32;
            object_offset += group.len;

            *draw_call = match meshes.get(separated_key.mesh_id) {
                Some(mesh) => {
                    GpuDrawCall {
                        index_count: mesh.index_count as u32,
                        instance_count: 0,
                        first_index: mesh.block.index_block().base(),
                        vertex_offset: mesh.block.vertex_block().base() as i32,
                        first_instance,
                        // TODO: For culling
                        bounds: GpuObjectBounds {
                            center: Vec4::ZERO,
                            half_extents: Vec4::ZERO,
                        },
                    }
                }
                None => {
                    warn!(
                        "Attempted to render mesh with ID `{:?}` that did not exist. Skipping draw.",
                        separated_key.mesh_id,
                    );
                    GpuDrawCall {
                        index_count: 0,
                        instance_count: 0,
                        first_index: 0,
                        vertex_offset: 0,
                        first_instance: 0,
                        bounds: GpuObjectBounds {
                            center: Vec4::ZERO,
                            half_extents: Vec4::ZERO,
                        },
                    }
                }
            }
        }

        // Update bindings
        self.global
            .update_object_bindings(frame, objects.object_data(), &self.output_ids);
        self.draw_gen.update_bindings(
            frame,
            self.set.ids().len(),
            (&self.draw_calls, self.draw_call_buffer_idx(frame)),
            objects,
            &self.input_ids,
            &self.output_ids,
        );
    }

    pub fn render_hzb<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        let draw_calls_idx = self.last_draw_call_buffer_idx(frame);

        let mut state_tracker = RenderStateTracker::default();

        args.pass
            .bind_index_buffer(args.mesh_factory.get_index_buffer(), 0, 0, IndexData::TYPE);

        // Render opaque static geometry
        let non_transparent_count = self.set.static_group_ranges().opaque.len();

        state_tracker.render_groups(
            0,
            RenderArgs {
                pass_id: HIGH_Z_PASS_ID,
                frame,
                camera: args.camera,
                pass: args.pass,
                mesh_factory: args.mesh_factory,
                meshes: args.meshes,
                materials: args.materials,
            },
            &self.draw_calls,
            draw_calls_idx,
            self.set
                .groups()
                .iter()
                .take(non_transparent_count)
                .enumerate(),
        );
    }

    pub fn render_depth_prepass<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        let draw_calls_idx = self.draw_call_buffer_idx(frame);

        let mut state_tracker = RenderStateTracker::default();

        args.pass
            .bind_index_buffer(args.mesh_factory.get_index_buffer(), 0, 0, IndexData::TYPE);

        // Render opaque and alpha cut objects
        let non_transparent_count = self.set.static_group_ranges().opaque.len()
            + self.set.static_group_ranges().alpha_cutout.len()
            + self.set.dynamic_group_ranges().opaque.len()
            + self.set.dynamic_group_ranges().alpha_cutout.len();

        state_tracker.render_groups(
            0,
            RenderArgs {
                pass_id: DEPTH_PREPASS_PASS_ID,
                frame,
                camera: args.camera,
                pass: args.pass,
                mesh_factory: args.mesh_factory,
                meshes: args.meshes,
                materials: args.materials,
            },
            &self.draw_calls,
            draw_calls_idx,
            self.set
                .groups()
                .iter()
                .take(non_transparent_count)
                .enumerate(),
        );
    }

    pub fn render_opaque<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        let draw_calls_idx = self.draw_call_buffer_idx(frame);

        let mut state_tracker = RenderStateTracker::default();

        args.pass
            .bind_index_buffer(args.mesh_factory.get_index_buffer(), 0, 0, IndexData::TYPE);

        let non_transparent_count = self.set.static_group_ranges().opaque.len()
            + self.set.static_group_ranges().alpha_cutout.len()
            + self.set.dynamic_group_ranges().opaque.len()
            + self.set.dynamic_group_ranges().alpha_cutout.len();

        state_tracker.render_groups(
            0,
            RenderArgs {
                pass_id: OPAQUE_PASS_ID,
                frame,
                camera: args.camera,
                pass: args.pass,
                mesh_factory: args.mesh_factory,
                meshes: args.meshes,
                materials: args.materials,
            },
            &self.draw_calls,
            draw_calls_idx,
            self.set
                .groups()
                .iter()
                .take(non_transparent_count)
                .enumerate(),
        );

        // // Render transparent objects
        // state_tracker.render_groups(
        //     non_transparent_count,
        //     RenderArgs {
        //         pass_id: TRANSPARENT_PASS_ID,
        //         pass: args.pass,
        //         mesh_factory: args.mesh_factory,
        //         meshes: args.meshes,
        //         materials: args.materials,
        //     },
        //     &self.draw_calls,
        //     draw_calls_idx,
        //     self.set
        //         .groups()
        //         .iter()
        //         .enumerate()
        //         .skip(non_transparent_count),
        // );
    }
}
