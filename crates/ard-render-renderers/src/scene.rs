use ard_ecs::prelude::*;
use ard_math::{Vec2, Vec3A};
use ard_pal::prelude::{Context, RenderPass};
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{
    factory::MaterialFactory, material::MaterialResource,
    material_instance::MaterialInstanceResource,
};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
};
use ard_render_si::bindings::Layouts;
use ard_render_textures::{factory::TextureFactory, texture::TextureResource};

use crate::{
    bins::{DrawBins, RenderArgs},
    highz::HzbImage,
    ids::RenderIds,
    passes::{
        color::ColorPassSets, depth_prepass::DepthPrepassSets, hzb::HzbPassSets,
        transparent::TransparentPassSets, COLOR_ALPHA_CUTOFF_PASS_ID, COLOR_OPAQUE_PASS_ID,
        DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID, DEPTH_OPAQUE_PREPASS_PASS_ID, HIGH_Z_PASS_ID,
        TRANSPARENT_PASS_ID,
    },
};

pub const DEFAULT_INPUT_ID_CAP: usize = 1;
pub const DEFAULT_OUTPUT_ID_CAP: usize = 1;

/// Primary GPU driven scene renderer.
#[derive(Resource)]
pub struct SceneRenderer {
    ctx: Context,
    /// Object IDs.
    ids: RenderIds,
    /// Draw bins.
    bins: DrawBins,
    /// Object information.
    set: RenderableSet,
    hzb_pass_sets: HzbPassSets,
    depth_prepass_sets: DepthPrepassSets,
    color_sets: ColorPassSets,
    transparent_sets: TransparentPassSets,
}

pub struct SceneRenderArgs<'a, 'b> {
    pub pass: &'b mut RenderPass<'a>,
    pub render_area: Vec2,
    pub static_dirty: bool,
    pub lock_culling: bool,
    pub camera: &'a CameraUbo,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource>,
    pub materials: &'a ResourceAllocator<MaterialResource>,
}

impl SceneRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        Self {
            ctx: ctx.clone(),
            ids: RenderIds::new(ctx),
            bins: DrawBins::new(),
            set: RenderableSet::default(),
            hzb_pass_sets: HzbPassSets::new(ctx, layouts),
            depth_prepass_sets: DepthPrepassSets::new(ctx, layouts),
            color_sets: ColorPassSets::new(ctx, layouts),
            transparent_sets: TransparentPassSets::new(ctx, layouts),
        }
    }

    #[inline(always)]
    pub fn object_set(&self) -> &RenderableSet {
        &self.set
    }

    #[inline(always)]
    pub fn color_pass_sets_mut(&mut self) -> &mut ColorPassSets {
        &mut self.color_sets
    }

    #[inline(always)]
    pub fn transparent_pass_sets_mut(&mut self) -> &mut TransparentPassSets {
        &mut self.transparent_sets
    }

    pub fn upload(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        textures: &ResourceAllocator<TextureResource>,
        meshes: &ResourceAllocator<MeshResource>,
        materials: &ResourceAllocator<MaterialResource>,
        material_instances: &ResourceAllocator<MaterialInstanceResource>,
        view_location: Vec3A,
    ) {
        puffin::profile_function!();

        // Update the set with all objects to render
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .with_transparent()
            .update(
                view_location,
                objects,
                meshes,
                false,
                |_| true,
                |_| true,
                |_| true,
            );

        // Upload object IDs
        let _buffers_expanded = self.ids.upload(frame, objects.static_dirty(), &self.set);

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
            textures,
            meshes,
            materials,
            material_instances,
        );
    }

    pub fn update_bindings(&mut self, frame: Frame, objects: &RenderObjects, hzb_image: &HzbImage) {
        self.hzb_pass_sets
            .update_object_data_bindings(frame, objects.object_data(), &self.ids);

        self.depth_prepass_sets.update_object_data_bindings(
            frame,
            objects.object_data(),
            &self.ids,
        );
        self.depth_prepass_sets.update_hzb_binding(frame, hzb_image);

        self.color_sets
            .update_object_data_bindings(frame, objects.object_data(), &self.ids);

        self.transparent_sets
            .update_object_data_bindings(frame, objects.object_data(), &self.ids);
        self.transparent_sets.update_hzb_binding(frame, hzb_image);
    }

    pub fn render_hzb<'a>(&'a self, frame: Frame, args: SceneRenderArgs<'a, '_>) {
        // Render static opaque geometry
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: HIGH_Z_PASS_ID,
            lock_culling: args.lock_culling,
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

    pub fn render_depth_prepass<'a>(&'a self, frame: Frame, args: SceneRenderArgs<'a, '_>) {
        // Render opaque and alpha cut objects
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: DEPTH_OPAQUE_PREPASS_PASS_ID,
            frame,
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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

    pub fn render_opaque<'a>(&'a self, frame: Frame, args: SceneRenderArgs<'a, '_>) {
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: COLOR_OPAQUE_PASS_ID,
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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
            lock_culling: args.lock_culling,
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

    pub fn render_transparent<'a>(&'a self, frame: Frame, args: SceneRenderArgs<'a, '_>) {
        self.bins.render_transparent_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: TRANSPARENT_PASS_ID,
            lock_culling: args.lock_culling,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.transparent_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }
}
