use ard_ecs::prelude::*;
use ard_math::{Vec2, Vec3A};
use ard_pal::prelude::*;
use ard_render_base::{resource::ResourceAllocator, Frame};
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
use ard_render_si::{bindings::*, types::*};
use ard_render_textures::{factory::TextureFactory, texture::TextureResource};
use ordered_float::NotNan;

use crate::{
    bins::{DrawBins, RenderArgs},
    highz::HzbImage,
    ids::RenderIds,
    passes::{
        entities::EntityPassSets, ENTITIES_ALPHA_CUTOFF_PASS_ID, ENTITIES_OPAQUE_PASS_ID,
        ENTITIES_TRANSPARENT_PASS_ID,
    },
};

/// Event to send when you want to select and entity on the canvas.
/// Contained is a UV coordinate on the canvas to select at.
#[derive(Event, Clone, Copy)]
pub struct SelectEntity(pub Vec2);

/// Event sent by the renderer when an entity was selected on the canvas.
#[derive(Event, Clone, Copy)]
pub struct EntitySelected(pub Entity);

/// Primary GPU driven scene renderer.
#[derive(Resource)]
pub struct EntityIdRenderer {
    ctx: Context,
    ids: RenderIds,
    bins: DrawBins,
    set: RenderableSet,
    entity_pass_sets: EntityPassSets,
    entity_select_pipeline: ComputePipeline,
    entity_select_set: DescriptorSet,
    selected_entity: Buffer,
}

pub struct EntityIdRenderArgs<'a, 'b> {
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

const ENTITY_SELECT_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    unnormalize_coords: false,
    border_color: None,
};

impl EntityIdRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./entity_select.comp.spv")),
                debug_name: Some("entity_select_shader".into()),
            },
        )
        .unwrap();

        let entity_select_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.entity_select.clone()],
                module,
                work_group_size: (1, 1, 1),
                push_constants_size: Some(
                    std::mem::size_of::<GpuEntitySelectPushConstants>() as u32
                ),
                debug_name: Some("entity_select_pipeline".into()),
            },
        )
        .unwrap();

        let selected_entity = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<u32>() as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuToCpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("selected_entity_buffer".into()),
            },
        )
        .unwrap();

        let mut entity_select_set = DescriptorSet::new(
            ctx.clone(),
            DescriptorSetCreateInfo {
                layout: layouts.entity_select.clone(),
                debug_name: Some("entity_select_set".into()),
            },
        )
        .unwrap();

        entity_select_set.update(&[DescriptorSetUpdate {
            binding: ENTITY_SELECT_SET_DST_BINDING,
            array_element: 0,
            value: DescriptorValue::StorageBuffer {
                buffer: &selected_entity,
                array_element: 0,
            },
        }]);

        Self {
            ctx: ctx.clone(),
            ids: RenderIds::new(ctx),
            bins: DrawBins::new(),
            set: RenderableSet::default(),
            selected_entity,
            entity_pass_sets: EntityPassSets::new(ctx, layouts),
            entity_select_pipeline,
            entity_select_set,
        }
    }

    #[inline(always)]
    pub fn object_set(&self) -> &RenderableSet {
        &self.set
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

    pub fn update_bindings(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        hzb_image: &HzbImage,
        entity_image: &Texture,
    ) {
        self.entity_pass_sets
            .update_object_data_bindings(frame, objects.object_data(), &self.ids);
        self.entity_pass_sets.update_hzb_binding(frame, hzb_image);

        self.entity_select_set.update(&[DescriptorSetUpdate {
            binding: ENTITY_SELECT_SET_SRC_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: entity_image,
                array_element: 0,
                sampler: ENTITY_SELECT_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }])
    }

    pub fn render<'a>(&'a self, frame: Frame, args: EntityIdRenderArgs<'a, '_>) {
        // Render opaque and alpha cut objects
        self.bins.render_static_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: ENTITIES_OPAQUE_PASS_ID,
            frame,
            lock_culling: args.lock_culling,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.entity_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_opaque_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: ENTITIES_OPAQUE_PASS_ID,
            frame,
            camera: args.camera,
            lock_culling: args.lock_culling,
            render_area: args.render_area,
            global_set: self.entity_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_static_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: ENTITIES_ALPHA_CUTOFF_PASS_ID,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            lock_culling: args.lock_culling,
            global_set: self.entity_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_dynamic_alpha_cutoff_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: ENTITIES_ALPHA_CUTOFF_PASS_ID,
            frame,
            lock_culling: args.lock_culling,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.entity_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });

        self.bins.render_transparent_bins(RenderArgs {
            ctx: &self.ctx,
            pass_id: ENTITIES_TRANSPARENT_PASS_ID,
            lock_culling: args.lock_culling,
            frame,
            render_area: args.render_area,
            camera: args.camera,
            global_set: self.entity_pass_sets.get_set(frame),
            pass: args.pass,
            mesh_factory: args.mesh_factory,
            material_factory: args.material_factory,
            texture_factory: args.texture_factory,
            meshes: args.meshes,
            materials: args.materials,
        });
    }

    pub fn select_entity<'a>(&'a self, commands: &mut CommandBuffer<'a>, uv: Vec2) {
        commands.compute_pass(
            &self.entity_select_pipeline,
            Some("select_entity"),
            |pass| {
                let constants = [GpuEntitySelectPushConstants { uv }];
                pass.bind_sets(0, vec![&self.entity_select_set]);
                pass.push_constants(bytemuck::cast_slice(&constants));
                ComputePassDispatch::Inline(1, 1, 1)
            },
        );
    }

    // NOTE: This function will stall rendering when called.
    pub fn read_back_selected_entity(&self) -> Option<Entity> {
        let view = self.selected_entity.read(0).unwrap();
        let u32_slice: &[u32] = bytemuck::cast_slice(view.as_ref());
        Entity::try_from(u32_slice[0])
            .ok()
            .filter(|e| *e != Entity::null())
    }
}
