use ard_ecs::resource::Resource;
use ard_pal::prelude::*;
use std::sync::{Arc, Mutex};

use crate::{
    material::{
        Material, MaterialCreateInfo, MaterialInner, MaterialInstance, MaterialInstanceCreateInfo,
        MaterialInstanceInner,
    },
    mesh::{Mesh, MeshCreateInfo, MeshInner},
};

use self::{
    allocator::ResourceAllocator,
    materials::MaterialBuffers,
    meshes::MeshBuffers,
    staging::{Staging, StagingRequest, StagingResource},
};

pub mod allocator;
pub mod materials;
pub mod meshes;
pub mod staging;

pub use ard_pal::prelude::{Shader, ShaderCreateError, ShaderCreateInfo};

pub const MAX_MATERIALS: usize = 512;
pub const MAX_MATERIAL_INSTANCES: usize = 2048;
pub const MAX_MESHES: usize = 2048;

#[derive(Clone, Resource)]
pub struct Factory(pub(crate) Arc<FactoryInner>);

pub(crate) struct FactoryInner {
    ctx: Context,
    pub layouts: Layouts,
    pub material_buffers: Mutex<MaterialBuffers>,
    pub mesh_buffers: Mutex<MeshBuffers>,
    pub staging: Mutex<Staging>,
    pub materials: Mutex<ResourceAllocator<MaterialInner>>,
    pub material_instances: Mutex<ResourceAllocator<MaterialInstanceInner>>,
    pub meshes: Mutex<ResourceAllocator<MeshInner>>,
}

pub(crate) struct Layouts {
    pub global: DescriptorSetLayout,
    pub materials: DescriptorSetLayout,
}

impl Factory {
    pub(crate) fn new(ctx: Context) -> Self {
        // Create container
        let material_buffers = MaterialBuffers::new(ctx.clone());
        let mesh_buffers = MeshBuffers::new(ctx.clone());
        let staging = Staging::new(ctx.clone());

        // Create layouts
        let global = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Object IDs
                    DescriptorBinding {
                        binding: 0,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Object data
                    DescriptorBinding {
                        binding: 1,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                ],
            },
        )
        .unwrap();

        let layouts = Layouts {
            global,
            materials: material_buffers.layout().clone(),
        };

        Factory(Arc::new(FactoryInner {
            ctx,
            layouts,
            material_buffers: Mutex::new(material_buffers),
            mesh_buffers: Mutex::new(mesh_buffers),
            staging: Mutex::new(staging),
            materials: Mutex::new(ResourceAllocator::new(MAX_MATERIALS)),
            material_instances: Mutex::new(ResourceAllocator::new(MAX_MATERIAL_INSTANCES)),
            meshes: Mutex::new(ResourceAllocator::new(MAX_MESHES)),
        }))
    }

    pub(crate) fn process(&self, frame: usize) {
        let mut staging = self.0.staging.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();
        let mut mesh_buffers = self.0.mesh_buffers.lock().unwrap();
        let mut materials = self.0.materials.lock().unwrap();
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut meshes = self.0.meshes.lock().unwrap();

        // Check if any uploads are complete, and if they are handle them appropriately
        staging.flush_complete_uploads(false, |resc| match resc {
            StagingResource::Mesh(id) => {
                if let Some(mesh) = meshes.get_mut(id) {
                    mesh.ready = true;
                }
            }
        });

        // Check for any new upload requests
        staging.upload(&mut mesh_buffers);

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
            },
            |_, _| {},
        );
    }

    #[inline]
    pub fn create_shader(
        &self,
        create_info: ShaderCreateInfo,
    ) -> Result<Shader, ShaderCreateError> {
        Shader::new(self.0.ctx.clone(), create_info)
    }

    #[inline]
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

    #[inline]
    pub fn create_material_instance(
        &self,
        create_info: MaterialInstanceCreateInfo,
    ) -> MaterialInstance {
        let mut material_instances = self.0.material_instances.lock().unwrap();
        let mut material_buffers = self.0.material_buffers.lock().unwrap();

        let material = create_info.material.clone();
        let inner = MaterialInstanceInner::new(&mut material_buffers, create_info);
        let escaper = material_instances.insert(inner);

        MaterialInstance {
            id: escaper.id(),
            escaper,
            factory: self.clone(),
            material,
        }
    }

    #[inline]
    pub fn create_mesh(&self, create_info: MeshCreateInfo) -> Mesh {
        let mut staging = self.0.staging.lock().unwrap();
        let mut mesh_buffers = self.0.mesh_buffers.lock().unwrap();
        let mut meshes = self.0.meshes.lock().unwrap();

        let (mesh_inner, vertex_staging, index_staging) =
            MeshInner::new(&self.0.ctx, &mut mesh_buffers, create_info);
        let layout_key = mesh_inner.layout.make_layout_key();
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
            layout_key,
        }
    }
}
