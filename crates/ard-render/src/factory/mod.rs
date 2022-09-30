use ard_ecs::resource::Resource;
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use std::sync::{Arc, Mutex};

use crate::{
    camera::{Camera, CameraDescriptor, CameraInner},
    material::{
        Material, MaterialCreateInfo, MaterialInner, MaterialInstance, MaterialInstanceCreateInfo,
        MaterialInstanceInner,
    },
    mesh::{Mesh, MeshCreateInfo, MeshInner},
    renderer::{occlusion::HzbGlobal, render_data::GlobalRenderData},
    texture::{Texture, TextureCreateInfo, TextureInner},
};

use self::{
    allocator::{ResourceAllocator, ResourceId},
    materials::MaterialBuffers,
    meshes::MeshBuffers,
    staging::{Staging, StagingRequest, StagingResource},
    textures::TextureSets,
};

pub mod allocator;
pub mod materials;
pub mod meshes;
pub mod staging;
pub mod textures;

pub use ard_pal::prelude::{Shader, ShaderCreateError, ShaderCreateInfo};

pub const MAX_MATERIALS: usize = 512;
pub const MAX_MATERIAL_INSTANCES: usize = 2048;
pub const MAX_MESHES: usize = 2048;
pub const MAX_TEXTURES: usize = 2048;
pub const MAX_CAMERAS: usize = 32;

#[derive(Clone, Resource)]
pub struct Factory(pub(crate) Arc<FactoryInner>);

pub(crate) struct FactoryInner {
    ctx: Context,
    pub layouts: Layouts,
    pub hzb: HzbGlobal,
    pub material_buffers: Mutex<MaterialBuffers>,
    pub mesh_buffers: Mutex<MeshBuffers>,
    pub texture_sets: Mutex<TextureSets>,
    pub staging: Mutex<Staging>,
    pub materials: Mutex<ResourceAllocator<MaterialInner>>,
    pub material_instances: Mutex<ResourceAllocator<MaterialInstanceInner>>,
    pub meshes: Mutex<ResourceAllocator<MeshInner>>,
    pub textures: Mutex<ResourceAllocator<TextureInner>>,
    pub cameras: Mutex<ResourceAllocator<CameraInner>>,
    pub active_cameras: Mutex<Vec<ResourceId>>,
}

pub(crate) struct Layouts {
    pub global: DescriptorSetLayout,
    pub camera: DescriptorSetLayout,
    pub textures: DescriptorSetLayout,
    pub materials: DescriptorSetLayout,
    pub draw_gen: DescriptorSetLayout,
    pub light_cluster: DescriptorSetLayout,
    pub froxel_gen: DescriptorSetLayout,
}

impl Factory {
    pub(crate) fn new(
        ctx: Context,
        anisotropy: Option<AnisotropyLevel>,
        global_data: &GlobalRenderData,
    ) -> Self {
        // Create containers
        let material_buffers = MaterialBuffers::new(ctx.clone());
        let mesh_buffers = MeshBuffers::new(ctx.clone());
        let staging = Staging::new(ctx.clone());
        let texture_sets = TextureSets::new(ctx.clone(), anisotropy);

        let layouts = Layouts {
            global: global_data.global_layout.clone(),
            camera: global_data.camera_layout.clone(),
            textures: texture_sets.layout().clone(),
            materials: material_buffers.layout().clone(),
            draw_gen: global_data.draw_gen_layout.clone(),
            light_cluster: global_data.light_cluster_layout.clone(),
            froxel_gen: global_data.froxel_gen_layout.clone(),
        };

        let hzb = HzbGlobal::new(&ctx);

        Factory(Arc::new(FactoryInner {
            ctx,
            layouts,
            hzb,
            material_buffers: Mutex::new(material_buffers),
            mesh_buffers: Mutex::new(mesh_buffers),
            staging: Mutex::new(staging),
            texture_sets: Mutex::new(texture_sets),
            materials: Mutex::new(ResourceAllocator::new(MAX_MATERIALS)),
            material_instances: Mutex::new(ResourceAllocator::new(MAX_MATERIAL_INSTANCES)),
            meshes: Mutex::new(ResourceAllocator::new(MAX_MESHES)),
            textures: Mutex::new(ResourceAllocator::new(MAX_TEXTURES)),
            cameras: Mutex::new(ResourceAllocator::new(MAX_CAMERAS)),
            active_cameras: Mutex::new(Vec::with_capacity(MAX_CAMERAS)),
        }))
    }

    pub(crate) fn process(&self, frame: usize) {
        let mut staging = self.0.staging.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();
        let mut texture_sets = self.0.texture_sets.lock().unwrap();
        let mut mesh_buffers = self.0.mesh_buffers.lock().unwrap();
        let mut materials = self.0.materials.lock().unwrap();
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut meshes = self.0.meshes.lock().unwrap();
        let mut textures = self.0.textures.lock().unwrap();
        let mut cameras = self.0.cameras.lock().unwrap();
        let mut active_cameras = self.0.active_cameras.lock().unwrap();

        // Check if any uploads are complete, and if they are handle them appropriately
        staging.flush_complete_uploads(false, |resc| match resc {
            StagingResource::Mesh(id) => {
                if let Some(mesh) = meshes.get_mut(id) {
                    mesh.ready = true;
                }
            }
            StagingResource::Texture(id) => texture_sets.texture_ready(id),
        });

        // Check for any new upload requests
        staging.upload(&mut mesh_buffers, &mut textures);

        // Flush material UBOs
        material_buffers.flush(&material_instances, frame);

        // Drop any pending resources
        meshes.drop_pending(
            frame,
            |_, mesh| {
                mesh_buffers.get_index_buffer_mut().free(mesh.index_block);
                mesh_buffers
                    .get_vertex_buffer_mut(mesh.layout)
                    .free(mesh.vertex_block);
            },
            |_, _| {},
        );
        materials.drop_pending(frame, |_, _| {}, |_, _| {});
        material_instances.drop_pending(
            frame,
            |_, material| {
                if let Some(block) = material.material_block {
                    material_buffers.free_ubo(material.data.len() as u64, block);
                }
                if let Some(block) = material.texture_block {
                    material_buffers.free_textures(block);
                }
            },
            |_, _| {},
        );
        textures.drop_pending(
            frame,
            |_, _| {},
            |id, _| {
                texture_sets.texture_dropped(id);
            },
        );
        cameras.drop_pending(
            frame,
            |_, _| {},
            |id, _| {
                // Find the location of the camera in the list
                let mut loc = None;
                for (i, found_id) in active_cameras.iter().enumerate() {
                    if *found_id == id {
                        loc = Some(i);
                        break;
                    }
                }

                // Remove the camera from the list
                if let Some(i) = loc {
                    active_cameras.remove(i);
                }
            },
        );

        texture_sets.update_set(frame, &textures);
    }

    #[inline]
    pub fn create_shader(
        &self,
        create_info: ShaderCreateInfo,
    ) -> Result<Shader, ShaderCreateError> {
        Shader::new(self.0.ctx.clone(), create_info)
    }

    pub fn create_material(&self, create_info: MaterialCreateInfo) -> Material {
        let mut materials = self.0.materials.lock().unwrap();
        let data_size = create_info.data_size;
        let texture_count = create_info.texture_count;
        let material = MaterialInner::new(&self.0.ctx, create_info, &self.0.layouts);
        let escaper = materials.insert(material);

        Material {
            id: escaper.id(),
            escaper,
            data_size,
            texture_count,
        }
    }

    pub fn create_material_instance(
        &self,
        create_info: MaterialInstanceCreateInfo,
    ) -> MaterialInstance {
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();

        let material = create_info.material.clone();
        let inner = MaterialInstanceInner::new(&mut material_buffers, create_info);
        let escaper = material_instances.insert(inner);

        let instance = MaterialInstance {
            id: escaper.id(),
            escaper,
            factory: self.clone(),
            material,
        };

        // Mark as dirty
        material_buffers.mark_dirty(instance.clone());

        instance
    }

    pub fn create_mesh(&self, create_info: MeshCreateInfo) -> Mesh {
        let mut staging = self.0.staging.lock().unwrap();
        let mut mesh_buffers = self.0.mesh_buffers.lock().unwrap();
        let mut meshes = self.0.meshes.lock().unwrap();

        let (mesh_inner, vertex_staging, index_staging) =
            MeshInner::new(&self.0.ctx, &mut mesh_buffers, create_info);
        let vertex_count = mesh_inner.vertex_count;
        let vertex_dst = mesh_inner.vertex_block;
        let index_dst = mesh_inner.index_block;
        let layout = mesh_inner.layout;

        let escaper = meshes.insert(mesh_inner);

        staging.add(StagingRequest::Mesh {
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
            escaper,
            layout,
        }
    }

    pub fn create_texture(&self, create_info: TextureCreateInfo) -> Texture {
        let mut staging = self.0.staging.lock().unwrap();
        let mut textures = self.0.textures.lock().unwrap();

        let mip_type = create_info.mip_type;
        let (texture, staging_buffer) = TextureInner::new(&self.0.ctx, create_info);
        let escaper = textures.insert(texture);

        let handle = Texture {
            id: escaper.id(),
            escaper,
        };

        staging.add(StagingRequest::Texture {
            id: handle.id,
            staging_buffer,
            mip_type,
        });

        handle
    }

    pub fn create_camera(&self, descriptor: CameraDescriptor) -> Camera {
        let mut cameras = self.0.cameras.lock().unwrap();
        let mut active_cameras = self.0.active_cameras.lock().unwrap();

        let order = descriptor.order;
        let camera = CameraInner::new(&self.0.ctx, descriptor, &self.0.layouts);
        let escaper = cameras.insert(camera);

        // Insert camera based on user provided ordering
        if let Err(loc) = active_cameras
            .binary_search_by_key(&order, |id| cameras.get(*id).unwrap().descriptor.order)
        {
            active_cameras.insert(loc, escaper.id());
        }

        Camera {
            id: escaper.id(),
            escaper,
        }
    }

    #[inline]
    pub fn update_camera(&self, camera: &Camera, descriptor: CameraDescriptor) {
        let mut cameras = self.0.cameras.lock().unwrap();
        let camera = cameras.get_mut(camera.id).unwrap();

        // Mark cluster regen if the perspective matrix changed
        if camera.descriptor.near != descriptor.near
            || camera.descriptor.far != descriptor.far
            || camera.descriptor.fov != descriptor.fov
        {
            camera.mark_froxel_regen();
        }

        camera.descriptor = descriptor;
    }

    pub fn update_material_data<T: Pod + Zeroable>(&self, material: &MaterialInstance, data: &T) {
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();
        let material_inner = material_instances.get_mut(material.id).unwrap();

        // Mark as dirty
        material_buffers.mark_dirty(material.clone());

        // Copy in the data
        material_inner
            .data
            .copy_from_slice(bytemuck::bytes_of(data));
    }

    pub fn update_material_texture(
        &self,
        material: &MaterialInstance,
        texture: Option<&Texture>,
        slot: usize,
    ) {
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();
        let material_inner = material_instances.get_mut(material.id).unwrap();

        // Mark as dirty
        material_buffers.mark_dirty(material.clone());

        // Copy in texture
        material_inner.textures[slot] = texture.map(|tex| tex.clone());
    }
}
