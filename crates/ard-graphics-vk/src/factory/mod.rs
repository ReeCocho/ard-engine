pub mod container;
pub mod descriptors;
pub mod layouts;
pub mod materials;
pub mod meshes;
pub mod staging;
pub mod textures;

use ash::vk;
use std::sync::{Arc, Mutex, RwLock, RwLockWriteGuard};

use crate::{
    alloc::Buffer, camera::forward_plus::GameRendererGraphRef, layouts::Layouts, prelude::*,
    renderer::forward_plus::Passes, renderer::graph::FRAMES_IN_FLIGHT, util::make_layout_key,
};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;

use self::{
    container::ResourceContainer,
    descriptors::DescriptorPool,
    materials::MaterialBuffers,
    meshes::MeshBuffers,
    staging::*,
    textures::{MipUpdate, TextureSets},
};

#[derive(Resource, Clone)]
pub struct Factory(pub(crate) Arc<FactoryInner>);

pub struct FactoryInner {
    ctx: GraphicsContext,
    staging: Mutex<StagingBuffers>,
    passes: Passes,
    graph: GameRendererGraphRef,
    main_camera: Mutex<Option<Camera>>,
    /// Used by the renderer to acquire exclusive access to the factory.
    exclusive: RwLock<()>,
    pub(crate) material_buffers: Mutex<MaterialBuffers>,
    pub(crate) mesh_buffers: Mutex<MeshBuffers>,
    pub(crate) texture_sets: Mutex<TextureSets>,
    pub(crate) camera_pool: Mutex<DescriptorPool>,
    pub(crate) layouts: Layouts,
    pub(crate) meshes: Mutex<ResourceContainer<MeshInner>>,
    pub(crate) shaders: Mutex<ResourceContainer<ShaderInner>>,
    pub(crate) pipelines: Mutex<ResourceContainer<PipelineInner>>,
    pub(crate) materials: Mutex<ResourceContainer<MaterialInner>>,
    pub(crate) cameras: Mutex<ResourceContainer<CameraInner>>,
    pub(crate) textures: Mutex<ResourceContainer<TextureInner>>,
}

impl Factory {
    pub(crate) unsafe fn new(
        ctx: &GraphicsContext,
        anisotropy: Option<AnisotropyLevel>,
        passes: &Passes,
        graph: &GameRendererGraphRef,
        global_layout: vk::DescriptorSetLayout,
    ) -> Self {
        let camera_pool = {
            let bindings = [vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .stage_flags(vk::ShaderStageFlags::ALL)
                .build()];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            DescriptorPool::new(
                ctx,
                &layout_create_info,
                FRAMES_IN_FLIGHT * VkBackend::MAX_CAMERA,
            )
        };

        let material_buffers = MaterialBuffers::new(ctx);
        let texture_sets = TextureSets::new(ctx, anisotropy);

        let layouts = Layouts::new(
            &ctx.0.device,
            global_layout,
            texture_sets.layout(),
            material_buffers.layout(),
            camera_pool.layout(),
        );

        // TODO: Make default vertex and index buffer lengths part of renderer settings
        let mesh_buffers = MeshBuffers::new(ctx, 1024, 1024);

        let factory = Factory(Arc::new(FactoryInner {
            ctx: ctx.clone(),
            passes: *passes,
            graph: graph.clone(),
            layouts,
            main_camera: Mutex::default(),
            material_buffers: Mutex::new(material_buffers),
            mesh_buffers: Mutex::new(mesh_buffers),
            texture_sets: Mutex::new(texture_sets),
            camera_pool: Mutex::new(camera_pool),
            exclusive: RwLock::default(),
            staging: Mutex::new(StagingBuffers::new(ctx)),
            meshes: Mutex::new(ResourceContainer::new()),
            shaders: Mutex::new(ResourceContainer::new()),
            pipelines: Mutex::new(ResourceContainer::new()),
            materials: Mutex::new(ResourceContainer::new()),
            cameras: Mutex::new(ResourceContainer::new()),
            textures: Mutex::new(ResourceContainer::new()),
        }));

        let camera = factory.create_camera(&CameraCreateInfo {
            descriptor: CameraDescriptor::default(),
        });

        *factory.0.main_camera.lock().unwrap() = Some(camera);

        factory
    }

    /// Process resources for the provided frame.
    pub(crate) unsafe fn process(&self, frame: usize) {
        let mut staging = self.0.staging.lock().expect("mutex poisoned");
        let mut texture_sets = self.0.texture_sets.lock().expect("mutex poisoned");
        let mut meshes = self.0.meshes.lock().expect("mutex poisoned");
        let mut materials = self.0.materials.lock().expect("mutex poisoned");
        let mut shaders = self.0.shaders.lock().expect("mutex poisoned");
        let mut pipelines = self.0.pipelines.lock().expect("mutex poisoned");
        let mut cameras = self.0.cameras.lock().expect("mutex poisoned");
        let mut textures = self.0.textures.lock().expect("mutex poisoned");
        let mut mesh_buffers = self.0.mesh_buffers.lock().expect("mutex poisoned");
        let mut material_buffers = self.0.material_buffers.lock().expect("mutex poisoned");

        // Check if any uploads are complete, and if they are handle them appropriately
        staging.flush_complete_uploads(false, &mut |id| match id {
            ResourceId::Mesh(id) => {
                if let Some(mesh) = meshes.get_mut(id) {
                    mesh.ready = true;
                }
            }
            ResourceId::Texture(id) => texture_sets.texture_ready(id),
            ResourceId::TextureMip {
                texture_id,
                mip_level,
            } => {
                if let Some(texture) = textures.get_mut(texture_id) {
                    // Update mip level
                    texture.loaded_mips |= 1 << mip_level;

                    // Create a new image view for the updated mip
                    let old_view = texture.create_new_view(&self.0.ctx.0.device);

                    // Tell texture set about the new view and pass ownership of the old view to it
                    // so the old view can be dropped.
                    texture_sets.texture_mip_update(MipUpdate {
                        id: texture_id,
                        old_view,
                        frame_to_drop: (frame + (FRAMES_IN_FLIGHT - 1)) % FRAMES_IN_FLIGHT,
                    });
                }
            }
        });

        // Check for any new upload requests and send them to the transfer queue
        staging.upload(&mut mesh_buffers);

        // Flush material UBOs
        material_buffers.flush(&materials, frame);

        // Drop any resources that are no longer referenced
        meshes.drop_pending(frame, &mut |_, mesh| {
            mesh_buffers.get_index_buffer().free(mesh.index_block);
            mesh_buffers
                .get_vertex_buffer(&mesh.layout)
                .free(mesh.vertex_block);
        });
        shaders.drop_pending(frame, &mut |_, _| {});
        pipelines.drop_pending(frame, &mut |_, _| {});
        materials.drop_pending(frame, &mut |_, material| {
            if let Some(idx) = material.material_slot {
                material_buffers.free_ubo(material.pipeline.inputs.ubo_size, idx);
            }

            if let Some(idx) = material.texture_slot {
                material_buffers.free_textures(idx);
            }
        });
        cameras.drop_pending(frame, &mut |_, _| {});
        textures.drop_pending(frame, &mut |id, _| texture_sets.texture_dropped(id));

        // Any texture uploads that are complete should signal to the texture sets that they are
        // available and must be bound to the primary set. Any dropped textures should signal
        // that they are gone and should be replaced with an error texture.
        texture_sets.update_sets(frame, &textures);
    }

    pub(crate) fn acquire(&self) -> RwLockWriteGuard<()> {
        self.0.exclusive.write().expect("lock poisoned")
    }
}

impl Drop for FactoryInner {
    fn drop(&mut self) {
        unsafe {
            self.layouts.release(&self.ctx.0.device);
        }
    }
}

impl FactoryApi<VkBackend> for Factory {
    fn create_mesh(&self, create_info: &MeshCreateInfo) -> Mesh {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut meshes = self.0.meshes.lock().expect("mutex poisoned");
        let mut mesh_buffers = self.0.mesh_buffers.lock().expect("mutex poisoned");
        assert!(meshes.len() < VkBackend::MAX_MESHES);

        let (mesh_inner, vertex_staging, index_staging) =
            unsafe { MeshInner::new(&self.0.ctx, &mut mesh_buffers, create_info) };

        let layout_key = make_layout_key(&mesh_inner.layout);
        let vertex_count = mesh_inner.vertex_count;
        let vertex_dst = mesh_inner.vertex_block;
        let index_dst = mesh_inner.index_block;
        let layout = mesh_inner.layout;

        let info = Arc::new(MeshInfo {
            index_count: mesh_inner.index_count,
            vertex_count: mesh_inner.vertex_count,
            bounds: mesh_inner.bounds,
            vertex_block: vertex_dst,
            index_block: index_dst,
        });

        let escaper = meshes.insert(mesh_inner);

        self.0
            .staging
            .lock()
            .expect("mutex poisoned")
            .add(StagingRequest::Mesh {
                id: escaper.id(),
                layout,
                vertex_count,
                vertex_staging,
                index_staging,
                vertex_dst,
                index_dst,
            });

        Mesh {
            id: escaper.id(),
            info,
            escaper,
            layout_key,
        }
    }

    fn create_shader(&self, create_info: &ShaderCreateInfo) -> Shader {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut shaders = self.0.shaders.lock().expect("mutex poisoned");

        let shader_inner = unsafe { ShaderInner::new(&self.0.ctx, create_info) };

        let ty = shader_inner.ty;
        let vertex_layout = shader_inner.vertex_layout;
        let inputs = shader_inner.inputs;
        let escaper = shaders.insert(shader_inner);

        Shader {
            id: escaper.id(),
            ty,
            vertex_layout,
            inputs,
            escaper,
        }
    }

    fn create_pipeline(&self, create_info: &PipelineCreateInfo<VkBackend>) -> Pipeline {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut pipelines = self.0.pipelines.lock().expect("mutex poisoned");
        let shaders = self.0.shaders.lock().expect("mutex poisoned");
        let graph = self.0.graph.lock().expect("mutex poisoned");

        let pipeline_inner = unsafe {
            PipelineInner::new(
                create_info,
                &self.0.ctx,
                &graph,
                &self.0.passes,
                &self.0.layouts,
                &shaders,
            )
        };
        let escaper = pipelines.insert(pipeline_inner);

        Pipeline {
            id: escaper.id(),
            inputs: create_info.vertex.inputs,
            escaper,
        }
    }

    fn create_material(&self, create_info: &MaterialCreateInfo<VkBackend>) -> Material {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut materials = self.0.materials.lock().expect("mutex poisoned");
        let mut material_buffers = self.0.material_buffers.lock().expect("mutex poisoned");

        let material_inner = unsafe { MaterialInner::new(&mut material_buffers, create_info) };
        let escaper = materials.insert(material_inner);

        Material {
            id: escaper.id(),
            pipeline_id: create_info.pipeline.id,
            escaper,
        }
    }

    fn create_camera(&self, create_info: &CameraCreateInfo) -> Camera {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut cameras = self.0.cameras.lock().expect("mutex poisoned");
        let mut camera_pool = self.0.camera_pool.lock().expect("mutex poisoned");

        let camera_inner = unsafe { CameraInner::new(&self.0.ctx, &mut camera_pool, create_info) };
        let escaper = cameras.insert(camera_inner);

        Camera {
            id: escaper.id(),
            escaper,
        }
    }

    fn create_texture(&self, create_info: &TextureCreateInfo) -> Texture {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut textures = self.0.textures.lock().expect("mutex poisoned");

        let (texture_inner, staging_buffer) =
            unsafe { TextureInner::new(&self.0.ctx, create_info) };
        let image_dst = texture_inner.image.clone();

        let escaper = textures.insert(texture_inner);

        self.0
            .staging
            .lock()
            .expect("mutex poisoned")
            .add(StagingRequest::Texture {
                id: escaper.id(),
                image_dst,
                staging_buffer,
                mip_type: create_info.mip_type,
            });

        Texture {
            id: escaper.id(),
            escaper,
        }
    }

    fn main_camera(&self) -> Camera {
        self.0
            .main_camera
            .lock()
            .expect("mutex poisoned")
            .as_ref()
            .unwrap()
            .clone()
    }

    fn update_camera(&self, camera: &Camera, descriptor: CameraDescriptor) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut cameras = self.0.cameras.lock().expect("mutex poisoned");
        let camera = cameras.get_mut(camera.id).unwrap();
        camera.descriptor = descriptor;
    }

    fn load_texture_mip(&self, texture: &Texture, level: usize, data: &[u8]) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let textures = self.0.textures.lock().expect("mutex poisoned");
        let mut staging = self.0.staging.lock().expect("mutex poisoned");

        let texture_inner = textures
            .get(texture.id)
            .expect("texture points to invalid texture");

        // Mip level must not already be loaded
        assert!(texture_inner.loaded_mips & (1 << level) == 0);

        // Create staging buffer for image data
        let staging_buffer = unsafe { Buffer::new_staging_buffer(&self.0.ctx, data) };

        // Send request to upload new mip
        staging.add(StagingRequest::TextureMipUpload {
            id: texture.id,
            image_dst: texture_inner.image.clone(),
            mip_level: level as u32,
            staging_buffer,
        });
    }

    fn update_material_data(&self, material: &Material, data: &[u8]) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut materials = self.0.materials.lock().expect("mutex poisoned");
        let mut material_buffers = self.0.material_buffers.lock().expect("mutex poisoned");
        let material_inner = materials.get_mut(material.id).unwrap();

        // Mark as dirty
        material_buffers
            .buffer_mut(material_inner.pipeline.inputs.ubo_size)
            .expect("invalid material buffer")
            .mark_dirty(material);

        // Copy in slice
        assert!(material_inner.material_data.len() >= data.len());
        for (i, byte) in data.iter().enumerate() {
            material_inner.material_data[i] = *byte;
        }
    }

    fn update_material_texture(&self, material: &Material, texture: &Texture, slot: usize) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");
        let mut materials = self.0.materials.lock().expect("mutex poisoned");
        let mut material_buffers = self.0.material_buffers.lock().expect("mutex poisoned");
        let material_inner = materials.get_mut(material.id).unwrap();

        // Mark as dirty
        material_buffers.texture_arrays_mut().mark_dirty(material);

        // Copy in slice
        assert!(slot < material_inner.textures.len());
        material_inner.textures[slot] = Some(texture.clone());
    }
}
