use std::ops::DerefMut;

use ard_ecs::resource::Resource;
use ard_formats::mesh::IndexData;
use ard_math::{Vec2, Vec3, Vec3A};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::{ubo::CameraUbo, Camera};
use ard_render_lighting::{
    lights::{Lighting, Lights},
    shadows::SunShadowsUbo,
};
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
    Model,
};
use ard_render_si::{bindings::Layouts, types::*};
use ard_render_textures::factory::TextureFactory;
use ordered_float::NotNan;
use rayon::prelude::*;

use crate::{
    bins::{DrawBins, RenderArgs},
    draw_gen::{DrawGenPipeline, DrawGenSets},
    global::{GlobalSetBindingUpdate, GlobalSets},
    SHADOW_PASS_ID,
};

pub const DEFAULT_INPUT_ID_CAP: usize = 1;
pub const DEFAULT_OUTPUT_ID_CAP: usize = 1;
pub const SHADOW_MAP_FORMAT: Format = Format::D32Sfloat;

pub(crate) const SHADOW_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToBorder,
    address_v: SamplerAddressMode::ClampToBorder,
    address_w: SamplerAddressMode::ClampToBorder,
    anisotropy: None,
    compare: Some(CompareOp::Less),
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: Some(BorderColor::FloatOpaqueWhite),
    unnormalize_coords: false,
};

/// Sun shadow cascades renderer.
#[derive(Resource)]
pub struct SunShadowsRenderer {
    input_ids: Buffer,
    set: RenderableSet,
    empty_shadow: Texture,
    ubo: Vec<SunShadowsUbo>,
    cascades: Vec<ShadowCascadeRenderData>,
}

pub struct ShadowRenderArgs<'a, 'b, const FIF: usize> {
    pub commands: &'b mut CommandBuffer<'a>,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory<FIF>,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource, FIF>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
}

struct ShadowCascadeRenderData {
    image: Texture,
    output_ids: Buffer,
    camera: CameraUbo,
    bins: DrawBins,
    draw_gen: DrawGenSets,
    global: GlobalSets,
}

impl SunShadowsRenderer {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        draw_gen: &DrawGenPipeline,
        frames_in_flight: usize,
        shadow_cascades: usize,
    ) -> Self {
        let empty_shadow = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: SHADOW_MAP_FORMAT,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("empty_shadow_map".into()),
            },
        )
        .unwrap();

        // Clear the empty shadow map
        let mut command_buffer = ctx.main().command_buffer();
        command_buffer.render_pass(
            RenderPassDescriptor {
                color_attachments: Vec::default(),
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    texture: &empty_shadow,
                    array_element: 0,
                    mip_level: 0,
                    load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }),
                depth_stencil_resolve_attachment: None,
            },
            |_| {},
        );
        ctx.main()
            .submit(Some("empty_shadow_prepare"), command_buffer)
            .wait_on(None);

        Self {
            empty_shadow,
            input_ids: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_INPUT_ID_CAP * std::mem::size_of::<GpuObjectId>()) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("sun_shadow_input_ids".to_owned()),
                },
            )
            .unwrap(),
            set: RenderableSet::default(),
            ubo: (0..frames_in_flight)
                .map(|_| SunShadowsUbo::new(ctx))
                .collect(),
            cascades: (0..shadow_cascades)
                .map(|_| ShadowCascadeRenderData::new(ctx, layouts, draw_gen, frames_in_flight))
                .collect(),
        }
    }

    #[inline]
    pub fn empty_shadow(&self) -> &Texture {
        &self.empty_shadow
    }

    #[inline]
    pub fn sun_shadow_info(&self, frame: Frame) -> &Buffer {
        self.ubo[usize::from(frame)].buffer()
    }

    #[inline]
    pub fn shadow_cascade(&self, i: usize) -> Option<&Texture> {
        self.cascades.get(i).map(|cascade| &cascade.image)
    }

    pub fn upload<const FIF: usize>(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource, FIF>,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        view_location: Vec3A,
    ) {
        // Update the set with all objects to render
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .update(view_location, objects, |_| true, |_| true, |_| true);

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

        // Update cascades
        let non_transparent_count = self.set.static_group_ranges().opaque.len()
            + self.set.static_group_ranges().alpha_cutout.len()
            + self.set.dynamic_group_ranges().opaque.len()
            + self.set.dynamic_group_ranges().alpha_cutout.len();

        self.cascades.par_iter_mut().for_each(|cascade| {
            // Expand output buffer if needed
            let output_id_buffer_size = (self.set.ids().len() * std::mem::size_of::<u32>()) as u64;
            if let Some(mut new_buffer) =
                Buffer::expand(&cascade.output_ids, output_id_buffer_size, false)
            {
                std::mem::swap(&mut cascade.output_ids, &mut new_buffer);
            }

            // Generate bins
            cascade
                .bins
                .preallocate_draw_call_buffers(self.set.groups().len());
            cascade.bins.gen_bins(
                frame,
                self.set.groups()[self.set.static_group_ranges().opaque.clone()].iter(),
                self.set.groups()[..non_transparent_count].iter(),
                std::iter::empty(),
                meshes,
                materials,
            );
        });
    }

    pub fn update_bindings<const FIF: usize>(
        &mut self,
        frame: Frame,
        lighting: &Lighting,
        objects: &RenderObjects,
        lights: &Lights,
    ) {
        self.cascades.iter_mut().for_each(|cascade| {
            cascade
                .global
                .update_object_bindings(GlobalSetBindingUpdate {
                    frame,
                    object_data: objects.object_data(),
                    object_ids: &cascade.output_ids,
                    global_lighting: lights.global_buffer(),
                    lights: lights.buffer(),
                    clusters: lighting.clusters(),
                    sun_shadow_info: self.ubo[usize::from(frame)].buffer(),
                    shadow_cascades: std::array::from_fn(|_| &self.empty_shadow),
                    // AO image isn't actually read during the shadow pass, so what we bind doesn't matter.
                    ao_image: &self.empty_shadow,
                });

            cascade.draw_gen.update_bindings::<FIF>(
                frame,
                self.set.ids().len(),
                self.set.non_transparent_object_count(),
                self.set.groups().len(),
                self.set.non_transparent_draw_count(),
                cascade.bins.src_draw_call_buffer(frame).unwrap(),
                cascade.bins.dst_draw_call_buffer(frame),
                cascade.bins.draw_counts_buffer(frame),
                objects,
                None,
                &self.input_ids,
                &cascade.output_ids,
            );
        });
    }

    pub fn update_cascade_views(
        &mut self,
        frame: Frame,
        camera: &Camera,
        camera_model: Model,
        screen_dims: (u32, u32),
        light_dir: Vec3,
    ) {
        self.ubo[usize::from(frame)].update(
            self.cascades.len(),
            light_dir,
            camera,
            camera_model,
            screen_dims.0 as f32 / screen_dims.1 as f32,
            4096,
        );

        self.cascades
            .iter_mut()
            .enumerate()
            .for_each(|(i, cascade)| {
                cascade
                    .camera
                    .update_raw(frame, self.ubo[usize::from(frame)].camera(i).unwrap());
            });
    }

    pub fn generate_draw_calls<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        pipeline: &DrawGenPipeline,
    ) {
        self.cascades.iter().for_each(|cascade| {
            pipeline.generate(
                frame,
                commands,
                &cascade.draw_gen,
                &cascade.camera,
                Vec2::ONE,
            )
        });
    }

    pub fn render<'a, const FIF: usize>(
        &'a self,
        frame: Frame,
        args: ShadowRenderArgs<'a, '_, FIF>,
    ) {
        self.cascades.iter().for_each(|cascade| {
            args.commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: Vec::default(),
                    color_resolve_attachments: Vec::default(),
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &cascade.image,
                        array_element: 0,
                        mip_level: 0,
                        load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                        store_op: StoreOp::Store,
                        samples: MultiSamples::Count1,
                    }),
                    depth_stencil_resolve_attachment: None,
                },
                |pass| {
                    pass.bind_index_buffer(
                        args.mesh_factory.get_index_buffer(),
                        0,
                        0,
                        IndexData::TYPE,
                    );

                    // Render opaque and alpha cut objects
                    cascade.bins.render_non_transparent_bins(RenderArgs {
                        pass_id: SHADOW_PASS_ID,
                        frame,
                        skip_texture_verify: true,
                        camera: &cascade.camera,
                        global: &cascade.global,
                        pass,
                        mesh_factory: args.mesh_factory,
                        material_factory: args.material_factory,
                        texture_factory: args.texture_factory,
                        meshes: args.meshes,
                        materials: args.materials,
                    });
                },
            );
        });
    }
}

impl ShadowCascadeRenderData {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        draw_gen: &DrawGenPipeline,
        frames_in_flight: usize,
    ) -> Self {
        Self {
            image: Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: SHADOW_MAP_FORMAT,
                    ty: TextureType::Type2D,
                    width: 4096,
                    height: 4096,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    sample_count: MultiSamples::Count1,
                    texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("shadow_cascade".into()),
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
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("sun_shadow_output_ids".to_owned()),
                },
            )
            .unwrap(),
            camera: CameraUbo::new(ctx, frames_in_flight, false, layouts),
            bins: DrawBins::new(ctx, frames_in_flight, true),
            global: GlobalSets::new(ctx, layouts, frames_in_flight),
            draw_gen: DrawGenSets::new(draw_gen, false, frames_in_flight),
        }
    }
}
