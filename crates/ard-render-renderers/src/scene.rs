use ard_ecs::prelude::*;
use ard_math::{Vec2, Vec3A};
use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, Context, MemoryUsage, QueueTypes, RenderPass,
    SharingMode,
};
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
};
use ard_render_si::{bindings::Layouts, types::GpuObjectId};
use ard_render_textures::factory::TextureFactory;
use std::ops::DerefMut;

use crate::{
    bins::{DrawBins, RenderArgs},
    highz::HzbImage,
    passes::{
        color::ColorPassSets, depth_only::DepthOnlyPassSets, depth_prepass::DepthPrepassSets,
        COLOR_ALPHA_CUTOFF_PASS_ID, COLOR_OPAQUE_PASS_ID, DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
        DEPTH_OPAQUE_PREPASS_PASS_ID, HIGH_Z_PASS_ID, TRANSPARENT_PASS_ID,
    },
};

pub const DEFAULT_INPUT_ID_CAP: usize = 1;
pub const DEFAULT_OUTPUT_ID_CAP: usize = 1;

/// Primary GPU driven scene renderer.
#[derive(Resource)]
pub struct SceneRenderer {
    ctx: Context,
    /// Object IDs which are filtered using the GPU driven frustum and occlusion culling compute
    /// shaders.
    input_ids: Buffer,
    /// Draw bins.
    bins: DrawBins,
    /// Object information.
    set: RenderableSet,
    /// Sets for rendering the HZB image.
    hzb_pass_sets: DepthOnlyPassSets,
    /// Sets for depth prepass rendering.
    depth_prepass_sets: DepthPrepassSets,
    /// Sets for color rendering.
    color_sets: ColorPassSets,
}

pub struct SceneRenderArgs<'a, 'b, const FIF: usize> {
    pub pass: &'b mut RenderPass<'a>,
    pub render_area: Vec2,
    pub static_dirty: bool,
    pub camera: &'a CameraUbo,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory<FIF>,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource, FIF>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
}

impl SceneRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts, frames_in_flight: usize) -> Self {
        Self {
            ctx: ctx.clone(),
            input_ids: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_INPUT_ID_CAP * std::mem::size_of::<GpuObjectId>()) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("input_ids".to_owned()),
                },
            )
            .unwrap(),
            bins: DrawBins::new(frames_in_flight),
            set: RenderableSet::default(),
            hzb_pass_sets: DepthOnlyPassSets::new(ctx, layouts, frames_in_flight),
            depth_prepass_sets: DepthPrepassSets::new(ctx, layouts, frames_in_flight),
            color_sets: ColorPassSets::new(ctx, layouts, frames_in_flight),
        }
    }

    #[inline(always)]
    pub fn color_pass_sets_mut(&mut self) -> &mut ColorPassSets {
        &mut self.color_sets
    }

    pub fn upload<const FIF: usize>(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource, FIF>,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        view_location: Vec3A,
    ) {
        puffin::profile_function!();

        // Update the set with all objects to render
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .with_transparent()
            .update(view_location, objects, meshes, |_| true, |_| true, |_| true);

        // Expand ID buffers if needed
        let input_id_buffer_size = std::mem::size_of_val(self.set.ids()) as u64;
        let input_id_buffer_expanded =
            match Buffer::expand(&self.input_ids, input_id_buffer_size, false) {
                Some(mut new_buffer) => {
                    std::mem::swap(&mut self.input_ids, &mut new_buffer);
                    true
                }
                None => false,
            };
        // Write in object IDs
        let mut id_view = self.input_ids.write(usize::from(frame)).unwrap();
        let id_slice = bytemuck::cast_slice_mut::<_, GpuObjectId>(id_view.deref_mut());

        // Write in static ids if they were modified
        if input_id_buffer_expanded || objects.static_dirty() {
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
        id_slice[self.set.transparent_object_range().clone()]
            .copy_from_slice(&self.set.ids()[self.set.transparent_object_range().clone()]);

        // Generate bins
        let non_transparent_count = self.set.static_group_ranges().opaque.len()
            + self.set.static_group_ranges().alpha_cutout.len()
            + self.set.dynamic_group_ranges().opaque.len()
            + self.set.dynamic_group_ranges().alpha_cutout.len();

        self.bins.gen_bins(
            frame,
            self.set.groups()[self.set.static_group_ranges().opaque.clone()].iter(),
            self.set.groups()[self.set.static_group_ranges().alpha_cutout.clone()].iter(),
            self.set.groups()[self.set.dynamic_group_ranges().opaque.clone()].iter(),
            self.set.groups()[self.set.dynamic_group_ranges().alpha_cutout.clone()].iter(),
            self.set.groups().iter().skip(non_transparent_count),
            meshes,
            materials,
        );
    }

    pub fn update_bindings<const FIF: usize>(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        hzb_image: &HzbImage<FIF>,
    ) {
        self.hzb_pass_sets.update_object_data_bindings(
            frame,
            objects.object_data(),
            &self.input_ids,
        );

        self.depth_prepass_sets.update_object_data_bindings(
            frame,
            objects.object_data(),
            &self.input_ids,
        );
        self.depth_prepass_sets.update_hzb_binding(frame, hzb_image);

        self.color_sets
            .update_object_data_bindings(frame, objects.object_data(), &self.input_ids);
    }

    pub fn render_hzb<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        // Render static opaque geometry
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: HIGH_Z_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.hzb_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }

    pub fn render_depth_prepass<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        // Render opaque and alpha cut objects
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: DEPTH_OPAQUE_PREPASS_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.depth_prepass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: DEPTH_OPAQUE_PREPASS_PASS_ID,
            frame,
            camera: args.camera,
            render_area: args.render_area,
            global_set: self.depth_prepass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_static_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.depth_prepass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.depth_prepass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }

    pub fn render_opaque<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: COLOR_OPAQUE_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.color_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: COLOR_OPAQUE_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.color_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_static_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: COLOR_ALPHA_CUTOFF_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.color_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: COLOR_ALPHA_CUTOFF_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.color_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }

    pub fn render_transparent<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: SceneRenderArgs<'a, '_, FIF>,
    ) {
        self.bins.render_transparent_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: TRANSPARENT_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.color_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }
}
