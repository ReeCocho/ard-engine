use std::sync::atomic::Ordering;

use super::{
    DrawCall, DrawGenInfo, DrawKey, ForwardPlus, InputObjectId, OutputObjectId, PointLightGenInfo,
    PointLightTable,
};
use crate::{
    alloc::{StorageBuffer, UniformBuffer, WriteStorageBuffer},
    camera::{
        depth_pyramid::{DepthPyramid, DepthPyramidGenerator},
        descriptors::DescriptorPool,
        forward_plus::{pick_depth_format, GameRendererGraph},
        graph::RenderPass,
        CameraLightClusters, Factory, GraphicsContext, Lighting, LightingUbo, PipelineType,
        StaticGeometry,
    },
    mesh::VertexLayoutKey,
    prelude::{graph::RenderGraphContext, CameraUbo},
    renderer::graph::GraphBuffer,
    shader_constants::{FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS, MAX_SHADOW_CASCADES},
};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::Vec2;
use ard_render_graph::{
    buffer::{BufferDescriptor, BufferId, BufferUsage},
    graph::{RenderGraphBuilder, RenderGraphResources},
    image::{ImageDescriptor, ImageId, SizeGroupId},
    pass::{ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, PassId},
};
use ash::vk;

use crate::VkBackend;

pub(crate) const DEFAULT_OBJECT_ID_BUFFER_CAP: usize = 1;
pub(crate) const DEFAULT_DRAW_CALL_CAP: usize = 1;

pub(crate) struct MeshPassCreateInfo {
    /// Is this pass toggleable?
    pub toggleable: bool,
    /// Size group used by this mesh pass. Must be shared between the depth and color images.
    pub size_group: SizeGroupId,
    /// Layers rendered by this pass.
    pub layers: RenderLayerFlags,
    /// A description of the camera used by this pass.
    pub camera: MeshPassCamera,
    /// Indicates if high-z culling is enabled or not.
    pub highz_culling: bool,
    /// Image to sample for shadow mapping.
    pub shadow_images: Option<[ImageId; MAX_SHADOW_CASCADES]>,
    /// Required depth image.
    pub depth_image: DepthStencilAttachmentDescriptor,
    /// The pipeline type used during depth prepass. This should only ever be values of
    /// `PipelineType::HighZRender` or `PipelineType::ShadowPass`.
    pub depth_pipeline_type: PipelineType,
    /// The pipeline type used during color rendering.
    pub color_pipeline_type: PipelineType,
    /// If `None`, rendering will be depth only.
    pub color_image: Option<ColorRendering>,
}

/// Used by the Forward+ renderer to draw objects.
pub(crate) struct MeshPass {
    /// Is this pass enabled?
    pub enabled: bool,
    /// Is this pass toggleable?
    pub toggleable: bool,
    /// Layers this mesh pass renders.
    pub layers: RenderLayerFlags,
    /// Depth image rendered to by this pass.
    pub depth_image: DepthStencilAttachmentDescriptor,
    /// Pass ID for depth prepass.
    pub depth_prepass_id: PassId,
    /// The pipeline type used during depth prepass. This should only ever be values of
    /// `PipelineType::HighZRender` or `PipelineType::ShadowPass`.
    pub depth_pipeline_type: PipelineType,
    /// The pipeline type used during color rendering.
    pub color_pipeline_type: PipelineType,
    /// Images to sample for shadow mapping.
    pub shadow_images: Option<[ImageId; MAX_SHADOW_CASCADES]>,
    /// The number of static objects that we rendered this frame.
    pub static_objects_rendered: usize,
    /// The number of static draw calls that we performed this frame.
    pub static_draw_calls: usize,
    /// The number of dynamic objects that we rendered this frame.
    pub dynamic_objects_rendered: usize,
    /// The number of dynamic draw calls that we performed this frame.
    pub dynamic_draw_calls: usize,
    /// Dynamic geometry query to look through when rendering.
    pub dynamic_geo_query: Option<ComponentQuery<(Read<Renderable<VkBackend>>,)>>,
    /// Camera to render from the perspective of.
    pub camera: MeshPassCameraInfo,
    /// Optional high-z culling information.
    pub highz_culling: Option<HighZCullingInfo>,
    /// Optional color rendering information.
    pub color_rendering: Option<ColorRenderingInfo>,
    /// Size group for the image drawn to by this renderer.
    pub size_group: SizeGroupId,
    /// Buffer that contains all draw calls.
    pub draw_calls: BufferId,
    /// Previous frames draw call buffer to use when generating the highz image.
    pub last_draw_calls: GraphBuffer,
    /// Buffer that contains the IDs of objects to render using this renderer.
    pub input_ids: BufferId,
    /// Output buffer containing IDs of unculled objects.
    pub output_ids: BufferId,
    /// Draw keys used during render sorting. One buffer per frame in flight.
    pub keys: Vec<Vec<(DrawKey, usize)>>,
    /// Set for draw call generation data. One per frame in flight.
    pub draw_gen_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    /// Set for global data for rendering. One per frame in flight.
    pub global_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    /// All sub passes within the mehs pass.
    pub passes: Vec<PassId>,
}

pub(crate) struct ColorRenderingInfo {
    /// Pass ID for opaque rendering.
    pub pass_id: PassId,
    /// Color image rendered to by this pass.
    pub color_image: ColorAttachmentDescriptor,
    /// Clustered lighting table.
    pub point_lights_table: BufferId,
    /// Set for light clustering. One per frame in flight.
    pub light_clustering_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
}

pub(crate) struct HighZCullingInfo {
    /// Pass ID for high-z culling.
    pub pass_id: PassId,
    /// Image to render to.
    pub image: ImageId,
    /// Depth pyramid image used for occlusion culling. One per frame in flight.
    pub depth_pyramids: Vec<DepthPyramid>,
}

pub(crate) struct MeshPassCameraInfo {
    /// Camera descriptor
    pub camera: MeshPassCamera,
    pub ubo: UniformBuffer,
    pub cluster_ssbo: StorageBuffer,
    pub aligned_cluster_size: u64,
    pub set: vk::DescriptorSet,
    /// Flag indicating this mesh pass needs it's camera SSBO regenerated. One flag per frame
    /// in flight.
    pub needs_ssbo_regen: [bool; FRAMES_IN_FLIGHT],
}

pub(crate) struct ColorRendering {
    pub color_image: ColorAttachmentDescriptor,
}

pub(crate) enum MeshPassCamera {
    /// Mesh pass should use the main camera.
    Main,
    /// Mesh pass uses a custom projection.
    Custom { ubo: CameraUbo },
}

// Methods
impl MeshPass {
    pub unsafe fn new(
        ctx: &GraphicsContext,
        lighting: &Lighting,
        brdf: (vk::ImageView, vk::Sampler),
        builder: &mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>,
        create_info: MeshPassCreateInfo,
        pyramid_gen: &mut DepthPyramidGenerator,
        camera_pool: &mut DescriptorPool,
        draw_gen_pool: &mut DescriptorPool,
        global_pool: &mut DescriptorPool,
        light_clustering_pool: &mut DescriptorPool,
    ) -> Self {
        let camera = MeshPassCameraInfo::new(ctx, create_info.camera, camera_pool);

        let mut keys = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            keys.push(Vec::default());
        }

        let mut draw_gen_sets = [vk::DescriptorSet::default(); FRAMES_IN_FLIGHT];
        for i in 0..FRAMES_IN_FLIGHT {
            draw_gen_sets[i] = draw_gen_pool.allocate();
        }

        let mut global_sets = [vk::DescriptorSet::default(); FRAMES_IN_FLIGHT];
        for i in 0..FRAMES_IN_FLIGHT {
            global_sets[i] = global_pool.allocate();

            // Bind stuff now that it is never recreated
            let clusters_info = [vk::DescriptorBufferInfo::builder()
                .offset(i as u64 * lighting.ubo.aligned_size())
                .range(std::mem::size_of::<LightingUbo>() as vk::DeviceSize)
                .buffer(lighting.ubo.buffer())
                .build()];

            let brdf_lut_info = [vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(brdf.0)
                .sampler(brdf.1)
                .build()];

            let writes = [
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .dst_set(global_sets[i])
                    .buffer_info(&clusters_info)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(10)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_set(global_sets[i])
                    .image_info(&brdf_lut_info)
                    .build(),
            ];

            ctx.0.device.update_descriptor_sets(&writes, &[]);
        }

        let draw_calls = builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<DrawCall>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let last_draw_calls = {
            let mut buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
            for _ in 0..FRAMES_IN_FLIGHT {
                buffers.push(WriteStorageBuffer::new(
                    ctx,
                    (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<DrawCall>()) as usize,
                ));
            }

            GraphBuffer::WriteStorage { buffers }
        };

        let input_ids = builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_OBJECT_ID_BUFFER_CAP * std::mem::size_of::<InputObjectId>()) as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let output_ids = builder.add_buffer(BufferDescriptor {
            size: (DEFAULT_OBJECT_ID_BUFFER_CAP * std::mem::size_of::<OutputObjectId>()) as u64,
            usage: BufferUsage::StorageBuffer,
        });

        let highz_culling = if create_info.highz_culling {
            Some(HighZCullingInfo::new(
                ctx,
                create_info.size_group,
                builder,
                pyramid_gen,
            ))
        } else {
            None
        };

        let color_rendering = match create_info.color_image {
            Some(color_rendering) => Some(ColorRenderingInfo::new(
                color_rendering.color_image,
                builder,
                light_clustering_pool,
            )),
            None => None,
        };

        Self {
            passes: Vec::default(),
            enabled: true,
            toggleable: create_info.toggleable,
            layers: create_info.layers,
            shadow_images: create_info.shadow_images,
            depth_image: create_info.depth_image,
            depth_pipeline_type: create_info.depth_pipeline_type,
            color_pipeline_type: create_info.color_pipeline_type,
            static_objects_rendered: 0,
            static_draw_calls: 0,
            dynamic_objects_rendered: 0,
            dynamic_draw_calls: 0,
            dynamic_geo_query: None,
            camera,
            size_group: create_info.size_group,
            highz_culling,
            color_rendering,
            draw_calls,
            last_draw_calls,
            input_ids,
            output_ids,
            keys,
            draw_gen_sets,
            global_sets,
            depth_prepass_id: PassId::invalid(),
        }
    }

    pub fn toggle_pass(&mut self, enabled: bool, graph: &mut GameRendererGraph) {
        self.enabled = enabled;
        for pass in &self.passes {
            graph.toggle_pass(*pass, enabled);
        }
    }

    pub unsafe fn update_global_set(
        &self,
        device: &ash::Device,
        frame: usize,
        shadow_sampler: vk::Sampler,
        poisson_disk_sampler: vk::Sampler,
        poisson_disk_view: vk::ImageView,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
        object_info: BufferId,
        point_lights: BufferId,
    ) {
        let global_set = self.global_sets[frame];

        // Write in light cluster if required
        if let Some(color_rendering) = &self.color_rendering {
            let clusters = resources
                .get_buffer(color_rendering.point_lights_table)
                .unwrap()
                .expect_write_storage(frame);

            let clusters_info = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(clusters.size())
                .buffer(clusters.buffer())
                .build()];

            let writes = [vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&clusters_info)
                .build()];

            device.update_descriptor_sets(&writes, &[]);
        }

        // Write shadow maps and poisson disk if needed
        if let Some(shadow_maps) = &self.shadow_images {
            let mut image_infos = [vk::DescriptorImageInfo::default(); MAX_SHADOW_CASCADES + 1];

            image_infos[0] = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(poisson_disk_view)
                .sampler(poisson_disk_sampler)
                .build();

            for i in 1..=MAX_SHADOW_CASCADES {
                let (_, shadow_map_images) = resources.get_image(shadow_maps[i - 1]).unwrap();
                image_infos[i] = vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(shadow_map_images[frame].view)
                    .sampler(shadow_sampler)
                    .build();
            }

            let mut writes = [vk::WriteDescriptorSet::default(); MAX_SHADOW_CASCADES + 1];

            writes[0] = vk::WriteDescriptorSet::builder()
                .dst_set(global_set)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_binding(6)
                .image_info(&image_infos[0..1])
                .build();

            for i in 1..=MAX_SHADOW_CASCADES {
                writes[i] = vk::WriteDescriptorSet::builder()
                    .dst_set(global_set)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .dst_binding(5)
                    .dst_array_element(i as u32 - 1)
                    .image_info(&image_infos[i..(i + 1)])
                    .build();
            }

            device.update_descriptor_sets(&writes, &[]);
        }

        // Write in everything else
        let object_info = resources
            .get_buffer(object_info)
            .unwrap()
            .expect_write_storage(frame);

        let point_lights = resources
            .get_buffer(point_lights)
            .unwrap()
            .expect_write_storage(frame);

        let output_idx = resources
            .get_buffer(self.output_ids)
            .unwrap()
            .expect_storage(frame);

        let input_infos = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(object_info.size())
            .buffer(object_info.buffer())
            .build()];

        let point_light_info = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(point_lights.size())
            .buffer(point_lights.buffer())
            .build()];

        let output_indices = [vk::DescriptorBufferInfo::builder()
            .offset(0)
            .range(output_idx.size())
            .buffer(output_idx.buffer())
            .build()];

        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&input_infos)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&point_light_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .dst_set(global_set)
                .buffer_info(&output_indices)
                .build(),
        ];

        device.update_descriptor_sets(&writes, &[]);
    }

    pub unsafe fn regen_depth_pyramids(
        &mut self,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
        depth_pyramid_gen: &mut DepthPyramidGenerator,
    ) {
        let highz_culling = match &mut self.highz_culling {
            Some(highz_culling) => highz_culling,
            None => return,
        };

        for pyramid in highz_culling.depth_pyramids.drain(..) {
            depth_pyramid_gen.free(pyramid);
        }

        let (width, height) = {
            let size_group = resources.get_size_group(self.size_group);
            (size_group.width, size_group.height)
        };

        for _ in 0..FRAMES_IN_FLIGHT {
            highz_culling
                .depth_pyramids
                .push(depth_pyramid_gen.allocate(width, height));
        }
    }

    pub unsafe fn release(&mut self, depth_pyramid_gen: &mut DepthPyramidGenerator) {
        if let Some(highz_culling) = &mut self.highz_culling {
            for depth_pyramid in highz_culling.depth_pyramids.drain(..) {
                depth_pyramid_gen.free(depth_pyramid);
            }
        }
    }
}

// Passes.
impl MeshPass {
    /// Sets up the camera UBO and generates the cameras cluster SSBO if needed.
    pub fn camera_setup(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame = ctx.frame();
        let device = &state.ctx.0.device;
        let mesh_pass = state.mesh_passes.get_active_pass_mut();

        // Update the camera UBO
        let ubo = match &mesh_pass.camera.camera {
            MeshPassCamera::Main => {
                let cameras = state.factory.0.cameras.read().expect("mutex poisoned");
                let main_camera = cameras.get(state.factory.main_camera().id).unwrap();
                let size_group = resources.get_size_group(mesh_pass.size_group);

                CameraUbo::new(
                    &main_camera.descriptor,
                    size_group.width as f32,
                    size_group.height as f32,
                )
            }
            MeshPassCamera::Custom { ubo } => *ubo,
        };

        unsafe {
            mesh_pass.camera.ubo.write(ubo, frame);
        }

        // Check for SSBO regen
        if !mesh_pass.color_rendering.is_some() || !mesh_pass.camera.needs_ssbo_regen[frame] {
            return;
        }

        mesh_pass.camera.needs_ssbo_regen[frame] = false;

        // Set up gen state
        let descriptor_sets = [mesh_pass.camera.set];
        let offsets = [
            mesh_pass.camera.ubo.aligned_size() as u32 * frame as u32,
            mesh_pass.camera.aligned_cluster_size as u32 * frame as u32,
        ];

        unsafe {
            device.cmd_bind_pipeline(
                *commands,
                vk::PipelineBindPoint::COMPUTE,
                state.mesh_passes.cluster_gen_pipeline,
            );

            device.cmd_bind_descriptor_sets(
                *commands,
                vk::PipelineBindPoint::COMPUTE,
                state.mesh_passes.cluster_gen_pipeline_layout,
                0,
                &descriptor_sets,
                &offsets,
            );
        }

        unsafe {
            // Dispatch for culling
            device.cmd_dispatch(
                *commands,
                FROXEL_TABLE_DIMS.0 as u32,
                FROXEL_TABLE_DIMS.1 as u32,
                FROXEL_TABLE_DIMS.2 as u32,
            );
        }

        // Barrier for highz generation
        let barrier = [vk::MemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .build()];

        unsafe {
            device.cmd_pipeline_barrier(
                *commands,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::BY_REGION,
                &barrier,
                &[],
                &[],
            );
        }
    }

    /// Renders a depth only image containing the geometry from last frame. Only used when high-z
    /// culling is enabled.
    pub fn highz_render(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame = ctx.frame();
        let device = &state.ctx.0.device;
        let mesh_pass = state.mesh_passes.get_active_pass();

        // We only draw if static geo isn't dirty because if it is dirty the model buffer
        // from last frame will be flushed, invalidating our draws. Also don't draw if the object
        // buffers were expanded because that will also invalidate our draws.
        if !state.static_geo.0.dirty[frame].load(Ordering::Relaxed)
            && !state.mesh_passes.object_buffers_expanded
        {
            unsafe {
                render(RenderArgs {
                    frame_idx: frame,
                    device,
                    draw_sky: false,
                    pipeline_type: PipelineType::HighZRender.idx(),
                    commands: *commands,
                    main_camera: &mesh_pass.camera,
                    skybox_pipeline: state.mesh_passes.skybox_pipeline,
                    skybox_pipeline_layout: state.mesh_passes.skybox_pipeline_layout,
                    factory: &state.factory,
                    global_set: mesh_pass.global_sets[frame],
                    draw_calls: resources
                        .get_buffer(mesh_pass.draw_calls)
                        .unwrap()
                        .expect_write_storage(frame)
                        .buffer(),
                    keys: &mesh_pass.keys[frame],
                    draw_offset: 0,
                    draw_count: mesh_pass.static_draw_calls,
                });
            }
        }
    }

    /// Generates the depth mip chain for occlussion culling. Only used when high-z culling is
    /// enabled.
    pub fn highz_generate(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let mesh_pass = state.mesh_passes.get_active_pass();

        let culling_info = mesh_pass.highz_culling.as_ref().unwrap();

        // Generate the depth pyramid for the frame
        unsafe {
            let depth_image = &resources.get_image(culling_info.image).unwrap().1[frame_idx];

            state.mesh_passes.depth_pyramid_gen.gen_pyramid(
                *commands,
                &depth_image.image,
                depth_image.view,
                &culling_info.depth_pyramids[frame_idx],
            );
        }
    }

    /// Prepares input ids for draw call generation.
    pub fn prepare_input_ids(
        &mut self,
        frame: usize,
        static_geometry: StaticGeometry,
        resources: &RenderGraphResources<RenderGraphContext<ForwardPlus>>,
        prepare_static: bool,
    ) {
        let keys = &mut self.keys[frame];

        let input_id_buffer = resources
            .get_buffer(self.input_ids)
            .unwrap()
            .expect_write_storage(frame);

        // Offset in the input ID buffer
        let mut cur_offset = 0;

        // Offset in the global object info buffer
        let mut info_offset = 0;

        // Prepare static geometry if needed
        if prepare_static {
            keys.clear();

            self.static_objects_rendered = 0;
            self.static_draw_calls = 0;

            let sorted_keys = static_geometry
                .0
                .sorted_keys
                .read()
                .expect("mutex poisoned");

            for key in sorted_keys.iter() {
                // Ignore if we have no matching layers
                let batch = static_geometry.0.batches.get(key).unwrap();
                if batch.layers & self.layers == RenderLayerFlags::EMPTY {
                    info_offset += batch.models.len();
                    continue;
                }

                // Write draw keys
                keys.push((*key, batch.models.len()));

                // Write in the input ids
                for i in 0..batch.models.len() {
                    unsafe {
                        input_id_buffer.write(
                            cur_offset,
                            InputObjectId {
                                info_idx: (info_offset + i) as u32,
                                batch_idx: [self.static_draw_calls as u32, 0],
                            },
                        );
                    }
                    cur_offset += 1;
                }

                self.static_draw_calls += 1;
                info_offset += batch.models.len();
            }

            self.static_objects_rendered = cur_offset;

            unsafe {
                input_id_buffer.flush(0, cur_offset * std::mem::size_of::<InputObjectId>());
            }
        } else {
            info_offset = static_geometry.0.len.load(Ordering::Relaxed);
            cur_offset = self.static_objects_rendered;
            keys.truncate(self.static_draw_calls);
        }

        // Write dynamic geometry
        let layers = self.layers;
        for (i, (renderable,)) in self
            .dynamic_geo_query
            .take()
            .unwrap()
            .into_iter()
            .enumerate()
            .filter(|(_, (renderable,))| {
                // Filter out objects that don't share at least one layer with us
                renderable.layers & layers != RenderLayerFlags::EMPTY
            })
        {
            unsafe {
                // Write object index
                // NOTE: Instead of writing the batch index here, we write the draw key so that we can
                // sort the object IDs here in place and then later determine the batch index. This is
                // fine as long as the size of object used for `batch_idx` is the same as the size used
                // for `DrawKey`.
                input_id_buffer.write(
                    cur_offset,
                    InputObjectId {
                        info_idx: (info_offset + i) as u32,
                        batch_idx: bytemuck::cast(crate::util::make_draw_key(
                            &renderable.material,
                            &renderable.mesh,
                        )),
                    },
                );
            }

            cur_offset += 1;
        }

        self.dynamic_objects_rendered = cur_offset - self.static_objects_rendered;

        // Sort the dymaic geometry portion of the input ids by the draw key we've written
        let id_slice = unsafe {
            std::slice::from_raw_parts_mut(
                (input_id_buffer.map().as_ptr() as *mut InputObjectId)
                    .add(self.static_objects_rendered),
                self.dynamic_objects_rendered,
            )
        };
        id_slice.sort_unstable_by_key(|id| bytemuck::cast::<[u32; 2], DrawKey>(id.batch_idx));

        // Convert draw keys into batch indices
        self.dynamic_draw_calls = 0;
        let mut cur_key = DrawKey::MAX;
        for id in id_slice {
            // New draw key means new draw
            let batch_as_key = bytemuck::cast(id.batch_idx);
            if batch_as_key != cur_key {
                cur_key = batch_as_key;
                keys.push((cur_key, 0));
                self.dynamic_draw_calls += 1;
            }

            // Update batch index and associated key size
            let draw_idx = self.static_draw_calls + (self.dynamic_draw_calls - 1);
            keys[draw_idx].1 += 1;
            id.batch_idx[0] = draw_idx as u32;
        }

        // Flush the dynamic portion of the input ids
        if self.dynamic_objects_rendered > 0 {
            unsafe {
                input_id_buffer.flush(
                    self.static_objects_rendered * std::mem::size_of::<InputObjectId>(),
                    self.dynamic_objects_rendered * std::mem::size_of::<InputObjectId>(),
                );
            }
        }
    }

    /// Prepares draw calls to be filled by the compute shader.
    pub fn prepare_draw_calls(
        &mut self,
        frame: usize,
        factory: Factory,
        resources: &RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let draw_call_buffer = resources
            .get_buffer(self.draw_calls)
            .unwrap()
            .expect_write_storage(frame);

        // Write draw calls
        let meshes = factory.0.meshes.read().expect("mutex poisoned");
        let mut curr_offset = 0;

        for (i, (key, draw_count)) in self.keys[frame].iter().enumerate() {
            let (_, _, mesh, _) = crate::util::from_draw_key(*key);
            let mesh = meshes.get(mesh).expect("invalid mesh");

            unsafe {
                draw_call_buffer.write(
                    i,
                    DrawCall {
                        indirect: vk::DrawIndexedIndirectCommand {
                            index_count: mesh.index_count as u32,
                            instance_count: 0,
                            first_index: mesh.index_block.base(),
                            vertex_offset: mesh.vertex_block.base() as i32,
                            first_instance: curr_offset as u32,
                        },
                        bounds: mesh.bounds,
                    },
                );
            }

            curr_offset += draw_count;
        }

        unsafe {
            draw_call_buffer.flush(
                0,
                (self.dynamic_draw_calls + self.static_draw_calls)
                    * std::mem::size_of::<DrawCall>(),
            );
        }
    }

    /// Generates draw calls using a compute shader.
    pub fn generate_draw_calls(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let device = &state.ctx.0.device;
        let object_info_id = state.mesh_passes.object_info;
        let sampler = state.mesh_passes.depth_pyramid_gen.sampler();
        let mesh_pass = state.mesh_passes.get_active_pass();

        let draw_gen_pipeline = if mesh_pass.highz_culling.is_some() {
            state.mesh_passes.draw_gen_pipeline
        } else {
            state.mesh_passes.draw_gen_no_highz_pipeline
        };

        // Update draw generation set
        unsafe {
            let set = mesh_pass.draw_gen_sets[frame_idx];

            let object_info_buffer = resources
                .get_buffer(object_info_id)
                .unwrap()
                .expect_write_storage(frame_idx);
            let input_id_buffer = resources
                .get_buffer(mesh_pass.input_ids)
                .unwrap()
                .expect_write_storage(frame_idx);
            let draw_call_buffer = resources
                .get_buffer(mesh_pass.draw_calls)
                .unwrap()
                .expect_write_storage(frame_idx);
            let output_id_buffer = resources
                .get_buffer(mesh_pass.output_ids)
                .unwrap()
                .expect_storage(frame_idx);

            let input_infos = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(object_info_buffer.size())
                .buffer(object_info_buffer.buffer())
                .build()];

            let input_ids = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(input_id_buffer.size())
                .buffer(input_id_buffer.buffer())
                .build()];

            let draw_calls = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(draw_call_buffer.size())
                .buffer(draw_call_buffer.buffer())
                .build()];

            let output_indices = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(output_id_buffer.size())
                .buffer(output_id_buffer.buffer())
                .build()];

            let writes = [
                // For draw gen
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&input_infos)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&input_ids)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&draw_calls)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&output_indices)
                    .build(),
            ];

            device.update_descriptor_sets(&writes, &[]);
        }

        // Update with depth pyramid if used
        if let Some(culling) = &mesh_pass.highz_culling {
            let set = mesh_pass.draw_gen_sets[frame_idx];

            let depth_img = [vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(culling.depth_pyramids[frame_idx].view())
                .sampler(sampler)
                .build()];

            let writes = [vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .dst_set(set)
                .image_info(&depth_img)
                .build()];

            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        // Compute shader for draw generation must wait on depth pyramid generation
        // NOTE: This also acts as a barrier for point light generation
        let barrier = [vk::MemoryBarrier::builder()
            .src_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .build()];

        unsafe {
            device.cmd_pipeline_barrier(
                *commands,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::BY_REGION,
                &barrier,
                &[],
                &[],
            );
        }

        // Set up culling state
        let descriptor_sets = [mesh_pass.draw_gen_sets[frame_idx], mesh_pass.camera.set];
        let offsets = [
            mesh_pass.camera.ubo.aligned_size() as u32 * frame_idx as u32,
            mesh_pass.camera.aligned_cluster_size as u32 * frame_idx as u32,
        ];

        unsafe {
            device.cmd_bind_pipeline(*commands, vk::PipelineBindPoint::COMPUTE, draw_gen_pipeline);
            device.cmd_bind_descriptor_sets(
                *commands,
                vk::PipelineBindPoint::COMPUTE,
                state.mesh_passes.draw_gen_pipeline_layout,
                0,
                &descriptor_sets,
                &offsets,
            );
        }

        // Determine the number of groups needed
        let object_count = mesh_pass.dynamic_objects_rendered + mesh_pass.static_objects_rendered;
        let group_count = if object_count as u32 % state.work_group_size != 0 {
            (object_count as u32 / state.work_group_size) + 1
        } else {
            object_count as u32 / state.work_group_size
        };

        let size_group = resources.get_size_group(mesh_pass.size_group);

        let draw_gen_info = [DrawGenInfo {
            object_count: object_count as u32,
            render_area: Vec2::new(size_group.width as f32, size_group.height as f32),
        }];

        unsafe {
            device.cmd_push_constants(
                *commands,
                state.mesh_passes.draw_gen_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::cast_slice(&draw_gen_info),
            );

            // Dispatch for culling
            device.cmd_dispatch(*commands, group_count, 1, 1);
        }
    }

    /// Generates point lights by clustering them in the camera cluster. This is disabled if there
    /// is no color image to render to.
    pub fn cluster_lights(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let device = &state.ctx.0.device;
        let point_lights_id = state.mesh_passes.point_lights;
        let mesh_pass = state.mesh_passes.get_active_pass();

        let color_rendering = mesh_pass.color_rendering.as_ref().unwrap();

        // Reset light counts
        let point_lights_table_buffer = resources
            .get_buffer_mut(color_rendering.point_lights_table)
            .unwrap()
            .expect_write_storage_mut(frame_idx);

        unsafe {
            point_lights_table_buffer.write_slice(
                0,
                &[0 as i32; FROXEL_TABLE_DIMS.0 * FROXEL_TABLE_DIMS.1 * FROXEL_TABLE_DIMS.2],
            );
            point_lights_table_buffer.flush(
                0,
                FROXEL_TABLE_DIMS.0
                    * FROXEL_TABLE_DIMS.1
                    * FROXEL_TABLE_DIMS.2
                    * std::mem::size_of::<i32>(),
            );
        }

        // Update point light generation set
        let set = color_rendering.light_clustering_sets[frame_idx];

        unsafe {
            let point_light_buffer = resources
                .get_buffer(point_lights_id)
                .unwrap()
                .expect_write_storage(frame_idx);

            let point_light_table_buffer = resources
                .get_buffer(color_rendering.point_lights_table)
                .unwrap()
                .expect_write_storage(frame_idx);

            let point_lights = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(point_light_buffer.size())
                .buffer(point_light_buffer.buffer())
                .build()];

            let point_light_table = [vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(point_light_table_buffer.size())
                .buffer(point_light_table_buffer.buffer())
                .build()];

            let writes = [
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&point_lights)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_array_element(0)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_set(set)
                    .buffer_info(&point_light_table)
                    .build(),
            ];

            device.update_descriptor_sets(&writes, &[]);
        }

        // Set up culling state
        let descriptor_sets = [set, mesh_pass.camera.set];
        let offsets = [
            mesh_pass.camera.ubo.aligned_size() as u32 * frame_idx as u32,
            mesh_pass.camera.aligned_cluster_size as u32 * frame_idx as u32,
        ];

        unsafe {
            device.cmd_bind_pipeline(
                *commands,
                vk::PipelineBindPoint::COMPUTE,
                state.mesh_passes.point_light_gen_pipeline,
            );
            device.cmd_bind_descriptor_sets(
                *commands,
                vk::PipelineBindPoint::COMPUTE,
                state.mesh_passes.point_light_gen_pipeline_layout,
                0,
                &descriptor_sets,
                &offsets,
            );
        }

        let light_gen_info = [PointLightGenInfo {
            light_count: state.mesh_passes.point_light_count as u32,
        }];

        unsafe {
            device.cmd_push_constants(
                *commands,
                state.mesh_passes.point_light_gen_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::cast_slice(&light_gen_info),
            );

            // Dispatch for culling
            device.cmd_dispatch(
                *commands,
                FROXEL_TABLE_DIMS.0 as u32,
                FROXEL_TABLE_DIMS.1 as u32,
                FROXEL_TABLE_DIMS.2 as u32,
            );
        }
    }

    /// Perform depth only rendering first to help minimize overdraw.
    pub fn depth_prepass(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let device = &state.ctx.0.device;
        let mesh_pass = state.mesh_passes.get_active_pass();

        unsafe {
            render(RenderArgs {
                frame_idx,
                device,
                draw_sky: false,
                pipeline_type: mesh_pass.depth_pipeline_type.idx(),
                commands: *commands,
                main_camera: &mesh_pass.camera,
                factory: &state.factory,
                skybox_pipeline: state.mesh_passes.skybox_pipeline,
                skybox_pipeline_layout: state.mesh_passes.skybox_pipeline_layout,
                global_set: mesh_pass.global_sets[frame_idx],
                draw_calls: resources
                    .get_buffer(mesh_pass.draw_calls)
                    .unwrap()
                    .expect_write_storage(frame_idx)
                    .buffer(),
                keys: &mesh_pass.keys[frame_idx],
                draw_offset: 0,
                draw_count: mesh_pass.static_draw_calls + mesh_pass.dynamic_draw_calls,
            });
        }
    }

    /// Perform rendering of opaque geometry.
    pub fn opaque_pass(
        ctx: &mut RenderGraphContext<ForwardPlus>,
        state: &mut ForwardPlus,
        commands: &vk::CommandBuffer,
        _pass: &mut RenderPass<ForwardPlus>,
        resources: &mut RenderGraphResources<RenderGraphContext<ForwardPlus>>,
    ) {
        let frame_idx = ctx.frame();
        let device = &state.ctx.0.device;
        let mesh_pass = state.mesh_passes.get_active_pass();

        unsafe {
            // Render geometry
            render(RenderArgs {
                frame_idx,
                device,
                pipeline_type: mesh_pass.color_pipeline_type.idx(),
                draw_sky: state.mesh_passes.draw_sky,
                skybox_pipeline: state.mesh_passes.skybox_pipeline,
                skybox_pipeline_layout: state.mesh_passes.skybox_pipeline_layout,
                commands: *commands,
                main_camera: &mesh_pass.camera,
                factory: &state.factory,
                global_set: mesh_pass.global_sets[frame_idx],
                draw_calls: resources
                    .get_buffer(mesh_pass.draw_calls)
                    .unwrap()
                    .expect_write_storage(frame_idx)
                    .buffer(),
                keys: &mesh_pass.keys[frame_idx],
                draw_offset: 0,
                draw_count: mesh_pass.static_draw_calls + mesh_pass.dynamic_draw_calls,
            });

            // Render debug objects
            let canvas_size = resources.get_size_group(state.canvas_size_group);

            state.debug_drawing.0.render(
                *commands,
                frame_idx,
                &mesh_pass.camera,
                (canvas_size.width, canvas_size.height),
            );
        }
    }
}

impl MeshPassCameraInfo {
    unsafe fn new(
        ctx: &GraphicsContext,
        camera: MeshPassCamera,
        camera_pool: &mut DescriptorPool,
    ) -> Self {
        // Create UBO
        let ubo = UniformBuffer::new(ctx, CameraUbo::default());

        // Create cluster SSBO
        let min_alignment = ctx.0.properties.limits.min_uniform_buffer_offset_alignment;
        let aligned_size = match min_alignment {
            0 => std::mem::size_of::<CameraLightClusters>() as u64,
            align => {
                let align_mask = align - 1;
                (std::mem::size_of::<CameraLightClusters>() as u64 + align_mask) & !align_mask
            }
        } as usize;

        let cluster_ssbo = StorageBuffer::new(ctx, aligned_size * FRAMES_IN_FLIGHT);

        let set = camera_pool.allocate();

        let buffer_infos = [
            vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(ubo.size())
                .buffer(ubo.buffer())
                .build(),
            vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(aligned_size as vk::DeviceSize)
                .buffer(cluster_ssbo.buffer())
                .build(),
        ];

        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .dst_set(set)
                .buffer_info(&buffer_infos[0..1])
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_array_element(0)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER_DYNAMIC)
                .dst_set(set)
                .buffer_info(&buffer_infos[1..2])
                .build(),
        ];

        ctx.0.device.update_descriptor_sets(&writes, &[]);

        MeshPassCameraInfo {
            camera,
            ubo,
            cluster_ssbo,
            aligned_cluster_size: aligned_size as u64,
            set,
            needs_ssbo_regen: [true; FRAMES_IN_FLIGHT],
        }
    }
}

impl HighZCullingInfo {
    unsafe fn new(
        ctx: &GraphicsContext,
        size_group: SizeGroupId,
        builder: &mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>,
        pyramid_gen: &mut DepthPyramidGenerator,
    ) -> Self {
        let image = builder.add_image(ImageDescriptor {
            format: pick_depth_format(ctx)
                .expect("unable to select a depth format for high-z culling"),
            size_group,
        });

        let (width, height) = {
            let size_group = builder.get_size_group(size_group);
            (size_group.width, size_group.height)
        };

        let mut depth_pyramids = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            depth_pyramids.push(pyramid_gen.allocate(width, height));
        }

        Self {
            image,
            depth_pyramids,
            pass_id: PassId::invalid(),
        }
    }
}

impl ColorRenderingInfo {
    unsafe fn new(
        color_image: ColorAttachmentDescriptor,
        builder: &mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>,
        light_clustering_pool: &mut DescriptorPool,
    ) -> Self {
        let point_lights_table = builder.add_buffer(BufferDescriptor {
            size: std::mem::size_of::<PointLightTable>() as u64,
            usage: BufferUsage::WriteStorageBuffer,
        });

        let mut light_clustering_sets = [vk::DescriptorSet::default(); FRAMES_IN_FLIGHT];
        for i in 0..FRAMES_IN_FLIGHT {
            light_clustering_sets[i] = light_clustering_pool.allocate();
        }

        Self {
            color_image,
            point_lights_table,
            light_clustering_sets,
            pass_id: PassId::invalid(),
        }
    }
}

struct RenderArgs<'a> {
    frame_idx: usize,
    device: &'a ash::Device,
    pipeline_type: usize,
    skybox_pipeline_layout: vk::PipelineLayout,
    skybox_pipeline: vk::Pipeline,
    draw_sky: bool,
    commands: vk::CommandBuffer,
    main_camera: &'a MeshPassCameraInfo,
    factory: &'a Factory,
    global_set: vk::DescriptorSet,
    draw_calls: vk::Buffer,
    draw_offset: usize,
    draw_count: usize,
    keys: &'a [(DrawKey, usize)],
}

unsafe fn render(args: RenderArgs) {
    let pipelines = args.factory.0.pipelines.read().expect("mutex poisoned");
    let mut mesh_buffers = args.factory.0.mesh_buffers.lock().expect("mutex poisoned");
    let meshes = args.factory.0.meshes.read().expect("mutex poisoned");
    let mut material_buffers = args
        .factory
        .0
        .material_buffers
        .lock()
        .expect("mutex poisoned");

    let mut last_pipeline = u32::MAX;
    let mut _last_material = u32::MAX;
    let mut last_mesh = u32::MAX;
    let mut last_vertex_layout = VertexLayoutKey::MAX;
    let mut last_ubo_size = u64::MAX;

    let mut sub_offset = 0;
    let mut sub_count = 0;
    let mut needs_draw = false;

    // Draw the skybox first if we are using a color pass
    if args.draw_sky && args.pipeline_type == PipelineType::OpaquePass.idx() {
        let sets = [args.global_set, args.main_camera.set];

        let offsets = [
            args.main_camera.ubo.aligned_size() as u32 * args.frame_idx as u32,
            args.main_camera.aligned_cluster_size as u32 * args.frame_idx as u32,
        ];

        args.device.cmd_bind_descriptor_sets(
            args.commands,
            vk::PipelineBindPoint::GRAPHICS,
            args.skybox_pipeline_layout,
            0,
            &sets,
            &offsets,
        );

        args.device.cmd_bind_pipeline(
            args.commands,
            vk::PipelineBindPoint::GRAPHICS,
            args.skybox_pipeline,
        );

        args.device.cmd_draw(args.commands, 36, 1, 0, 0);
    }

    // Bind global sets
    let sets = [
        args.global_set,
        args.factory
            .0
            .texture_sets
            .lock()
            .expect("mutex poisoned")
            .get_set(args.frame_idx),
        args.main_camera.set,
    ];

    let offsets = [
        args.main_camera.ubo.aligned_size() as u32 * args.frame_idx as u32,
        args.main_camera.aligned_cluster_size as u32 * args.frame_idx as u32,
    ];

    args.device.cmd_bind_descriptor_sets(
        args.commands,
        vk::PipelineBindPoint::GRAPHICS,
        args.factory.0.layouts.opaque_pipeline_layout,
        0,
        &sets,
        &offsets,
    );

    // Bind index buffers
    {
        let idx_buffer = mesh_buffers.get_index_buffer();
        args.device.cmd_bind_index_buffer(
            args.commands,
            idx_buffer.buffer(),
            0,
            vk::IndexType::UINT32,
        );
    }

    for (i, (key, _)) in args.keys[0..(args.draw_offset + args.draw_count)]
        .iter()
        .enumerate()
        .skip(args.draw_offset)
    {
        let (pipeline_id, vertex_layout, mesh_id, _) = crate::util::from_draw_key(*key);

        // Bind pipeline
        if last_pipeline != pipeline_id {
            let pipeline = pipelines.get(pipeline_id).expect("invalid pipeline");
            args.device.cmd_bind_pipeline(
                args.commands,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipelines[args.pipeline_type],
            );

            // Bind material data if needed
            if pipeline.inputs.ubo_size != last_ubo_size {
                let sets = [material_buffers
                    .get_set(pipeline.inputs.ubo_size, args.frame_idx)
                    .set];

                args.device.cmd_bind_descriptor_sets(
                    args.commands,
                    vk::PipelineBindPoint::GRAPHICS,
                    args.factory.0.layouts.opaque_pipeline_layout,
                    3,
                    &sets,
                    &[],
                );

                last_ubo_size = pipeline.inputs.ubo_size;
            }

            last_pipeline = pipeline_id;
            needs_draw = true;
        }

        // Bind vertex buffers
        if last_vertex_layout != vertex_layout {
            last_vertex_layout = vertex_layout;
            let vertex_layout = crate::util::from_layout_key(vertex_layout);
            let vbuffer = mesh_buffers.get_vertex_buffer(&vertex_layout);
            vbuffer.bind(args.device, args.commands, &vertex_layout);
            needs_draw = true;
        }

        // Check if current mesh is valid
        if last_mesh != mesh_id {
            let mesh = meshes.get(mesh_id).expect("invalid mesh");
            if !mesh.ready {
                needs_draw = true;
                continue;
            }
            last_mesh = mesh_id;
        }

        // Draw if needed
        if needs_draw {
            if sub_count > 0 {
                args.device.cmd_draw_indexed_indirect(
                    args.commands,
                    args.draw_calls,
                    (sub_offset * std::mem::size_of::<DrawCall>()) as u64,
                    sub_count,
                    std::mem::size_of::<DrawCall>() as u32,
                );
            }

            sub_count = 0;
            sub_offset = i;
            needs_draw = false;
        }

        // New draw
        sub_count += 1;
    }

    // Handle final draw
    if sub_count > 0 {
        args.device.cmd_draw_indexed_indirect(
            args.commands,
            args.draw_calls,
            (sub_offset * std::mem::size_of::<DrawCall>()) as u64,
            sub_count,
            std::mem::size_of::<DrawCall>() as u32,
        );
    }
}
