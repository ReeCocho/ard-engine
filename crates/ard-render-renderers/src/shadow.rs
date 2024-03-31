use ard_ecs::resource::Resource;
use ard_math::{Vec2, Vec3, Vec3A};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator, FRAMES_IN_FLIGHT};
use ard_render_camera::{ubo::CameraUbo, Camera};
use ard_render_lighting::shadows::{ShadowCascadeSettings, SunShadowsUbo};
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
    Model,
};
use ard_render_si::bindings::Layouts;
use ard_render_textures::factory::TextureFactory;
use ordered_float::NotNan;

use crate::{
    bins::{DrawBins, RenderArgs},
    ids::RenderIds,
    passes::{shadow::ShadowPassSets, SHADOW_ALPHA_CUTOFF_PASS_ID, SHADOW_OPAQUE_PASS_ID},
};

pub const DEFAULT_INPUT_ID_CAP: usize = 1;
pub const DEFAULT_OUTPUT_ID_CAP: usize = 1;
pub const SHADOW_MAP_FORMAT: Format = Format::D16Unorm;

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
    ctx: Context,
    ids: RenderIds,
    set: RenderableSet,
    empty_shadow: Texture,
    ubo: [SunShadowsUbo; FRAMES_IN_FLIGHT],
    bins: DrawBins,
    cascades: Vec<ShadowCascadeRenderData>,
}

pub struct ShadowRenderArgs<'a, 'b> {
    pub commands: &'b mut CommandBuffer<'a>,
    pub mesh_factory: &'a MeshFactory,
    pub material_factory: &'a MaterialFactory,
    pub texture_factory: &'a TextureFactory,
    pub meshes: &'a ResourceAllocator<MeshResource>,
    pub materials: &'a ResourceAllocator<MaterialResource>,
    pub cascade: usize,
}

struct ShadowCascadeRenderData {
    image: Texture,
    camera: CameraUbo,
    sets: ShadowPassSets,
}

impl SunShadowsRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts, shadow_cascades: usize) -> Self {
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
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Concurrent,
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
                    dst: DepthStencilAttachmentDestination::Texture {
                        texture: &empty_shadow,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }),
                depth_stencil_resolve_attachment: None,
            },
            None,
            |_| {},
        );
        ctx.main()
            .submit(Some("empty_shadow_prepare"), command_buffer)
            .wait_on(None);

        let ubo = std::array::from_fn(|_| SunShadowsUbo::new(ctx));

        let cascades = (0..shadow_cascades)
            .map(|_| ShadowCascadeRenderData::new(ctx, layouts, 1))
            .collect();

        Self {
            ctx: ctx.clone(),
            ids: RenderIds::new(ctx),
            set: RenderableSet::default(),
            ubo,
            bins: DrawBins::new(),
            cascades,
            empty_shadow,
        }
    }

    #[inline]
    pub fn cascade_count(&self) -> usize {
        self.cascades.len()
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

    pub fn upload(
        &mut self,
        frame: Frame,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource>,
        materials: &ResourceAllocator<MaterialResource>,
        view_location: Vec3A,
    ) {
        puffin::profile_function!();

        // Update the set with all objects to render
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .update(view_location, objects, meshes, |_| true, |_| true, |_| true);

        // Upload IDs
        let _buffer_expanded = self.ids.upload(frame, objects.static_dirty(), &self.set);

        // Generate bins
        self.bins.gen_bins(
            frame,
            self.set.groups()[self.set.static_group_ranges().opaque.clone()].iter(),
            self.set.groups()[self.set.static_group_ranges().alpha_cutout.clone()].iter(),
            self.set.groups()[self.set.dynamic_group_ranges().opaque.clone()].iter(),
            self.set.groups()[self.set.dynamic_group_ranges().alpha_cutout.clone()].iter(),
            std::iter::empty(),
            meshes,
            materials,
        );
    }

    pub fn update_cascade_settings(
        &mut self,
        ctx: &Context,
        layouts: &Layouts,
        cascades: &[ShadowCascadeSettings],
    ) -> bool {
        let mut needs_resize = self.cascades.len() != cascades.len();

        let mut i = cascades.len();
        self.cascades.resize_with(cascades.len(), || {
            let new_cascade = ShadowCascadeRenderData::new(ctx, layouts, cascades[i].resolution);
            i += 1;
            new_cascade
        });

        for (orig_cascade, new_cascade) in self.cascades.iter_mut().zip(cascades.iter()) {
            if orig_cascade.resize(ctx, new_cascade.resolution) {
                needs_resize = true;
            }
        }

        needs_resize
    }

    pub fn update_bindings(&mut self, frame: Frame, objects: &RenderObjects) {
        self.cascades.iter_mut().for_each(|cascade| {
            cascade
                .sets
                .update_object_data_bindings(frame, objects.object_data(), &self.ids);
        });
    }

    pub fn update_cascade_views(
        &mut self,
        frame: Frame,
        camera: &Camera,
        camera_model: Model,
        screen_dims: (u32, u32),
        light_dir: Vec3,
        cascades: &[ShadowCascadeSettings],
    ) {
        self.ubo[usize::from(frame)].update(
            cascades,
            light_dir,
            camera,
            camera_model,
            screen_dims.0 as f32 / screen_dims.1 as f32,
        );

        self.cascades
            .iter_mut()
            .enumerate()
            .for_each(|(i, cascade)| {
                cascade.camera.update_raw(
                    frame,
                    self.ubo[usize::from(frame)].camera(i).unwrap(),
                    0,
                );
            });
    }

    pub fn render<'a>(&'a self, frame: Frame, args: ShadowRenderArgs<'a, '_>) {
        let cascade = &self.cascades[args.cascade];
        let render_area = Vec2::new(cascade.image.dims().0 as f32, cascade.image.dims().1 as f32);
        args.commands.render_pass(
            RenderPassDescriptor {
                color_attachments: Vec::default(),
                color_resolve_attachments: Vec::default(),
                depth_stencil_attachment: Some(DepthStencilAttachment {
                    dst: DepthStencilAttachmentDestination::Texture {
                        texture: &cascade.image,
                        array_element: 0,
                        mip_level: 0,
                    },
                    load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                    store_op: StoreOp::Store,
                    samples: MultiSamples::Count1,
                }),
                depth_stencil_resolve_attachment: None,
            },
            Some("render_shadows"),
            |pass| {
                // Render opaque and alpha cut objects
                self.bins.render_static_opaque_bins(RenderArgs {
                    ctx: &self.ctx,
                    pass_id: SHADOW_OPAQUE_PASS_ID,
                    frame,
                    render_area,
                    lock_culling: false,
                    camera: &cascade.camera,
                    global_set: cascade.sets.get_set(frame),
                    pass,
                    mesh_factory: args.mesh_factory,
                    material_factory: args.material_factory,
                    texture_factory: args.texture_factory,
                    meshes: args.meshes,
                    materials: args.materials,
                });

                self.bins.render_dynamic_opaque_bins(RenderArgs {
                    ctx: &self.ctx,
                    pass_id: SHADOW_OPAQUE_PASS_ID,
                    frame,
                    render_area,
                    lock_culling: false,
                    camera: &cascade.camera,
                    global_set: cascade.sets.get_set(frame),
                    pass,
                    mesh_factory: args.mesh_factory,
                    material_factory: args.material_factory,
                    texture_factory: args.texture_factory,
                    meshes: args.meshes,
                    materials: args.materials,
                });

                self.bins.render_static_alpha_cutoff_bins(RenderArgs {
                    ctx: &self.ctx,
                    pass_id: SHADOW_ALPHA_CUTOFF_PASS_ID,
                    frame,
                    lock_culling: false,
                    render_area,
                    camera: &cascade.camera,
                    global_set: cascade.sets.get_set(frame),
                    pass,
                    mesh_factory: args.mesh_factory,
                    material_factory: args.material_factory,
                    texture_factory: args.texture_factory,
                    meshes: args.meshes,
                    materials: args.materials,
                });

                self.bins.render_dynamic_alpha_cutoff_bins(RenderArgs {
                    ctx: &self.ctx,
                    pass_id: SHADOW_ALPHA_CUTOFF_PASS_ID,
                    frame,
                    lock_culling: false,
                    render_area,
                    camera: &cascade.camera,
                    global_set: cascade.sets.get_set(frame),
                    pass,
                    mesh_factory: args.mesh_factory,
                    material_factory: args.material_factory,
                    texture_factory: args.texture_factory,
                    meshes: args.meshes,
                    materials: args.materials,
                });
            },
        );
    }
}

impl ShadowCascadeRenderData {
    pub fn new(ctx: &Context, layouts: &Layouts, resolution: u32) -> Self {
        Self {
            image: Self::create_image(ctx, resolution),
            camera: CameraUbo::new(ctx, false, layouts),
            sets: ShadowPassSets::new(ctx, layouts),
        }
    }

    pub fn resize(&mut self, ctx: &Context, resolution: u32) -> bool {
        if self.image.dims().0 == resolution {
            return false;
        }

        self.image = Self::create_image(ctx, resolution);
        true
    }

    fn create_image(ctx: &Context, resolution: u32) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: SHADOW_MAP_FORMAT,
                ty: TextureType::Type2D,
                width: resolution.max(1),
                height: resolution.max(1),
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                sharing_mode: SharingMode::Concurrent,
                debug_name: Some("shadow_cascade".into()),
            },
        )
        .unwrap()
    }
}
