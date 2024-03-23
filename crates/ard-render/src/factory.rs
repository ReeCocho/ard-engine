use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    staging::{Staging, StagingRequest, StagingResource},
    FRAMES_IN_FLIGHT,
};
use ard_ecs::prelude::*;
use ard_formats::{mesh::MeshData, texture::TextureSource};
use ard_pal::prelude::{Buffer, Context, QueueType};
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_material::{
    factory::{MaterialFactory, MaterialFactoryConfig},
    material::{Material, MaterialCreateError, MaterialCreateInfo, MaterialResource},
    material_instance::{
        MaterialInstance, MaterialInstanceCreateError, MaterialInstanceCreateInfo,
        MaterialInstanceResource, TextureSlot,
    },
    shader::{Shader, ShaderCreateError, ShaderCreateInfo, ShaderResource},
};
use ard_render_meshes::{
    factory::{MeshFactory, MeshFactoryConfig},
    mesh::{Mesh, MeshCreateError, MeshCreateInfo, MeshResource},
};
use ard_render_si::{
    bindings::{Layouts, MATERIALS_SET_DATA_BINDING, MATERIALS_SET_TEXTURE_SLOTS_BINDING},
    consts::*,
};
use ard_render_textures::{
    factory::{MipUpdate, TextureFactory, TextureMipUpload},
    texture::{Texture, TextureCreateError, TextureCreateInfo, TextureResource},
};
use bytemuck::{Pod, Zeroable};

// TODO: Make these configurable
pub const DROP_LATENCY: usize = 2;
pub const MAX_SHADERS: usize = 1024;
pub const MAX_MATERIALS: usize = 512;
pub const MAX_MATERIAL_INSTANCES: usize = 2048;
pub const MAX_MESHES: usize = 2048;
pub const MAX_CUBE_MAPS: usize = 128;
pub const MAX_CAMERAS: usize = 32;

#[derive(Clone, Resource)]
pub struct Factory {
    pbr_material: Material,
    pub(crate) inner: Arc<FactoryInner>,
}

pub(crate) struct FactoryInner {
    staging: Mutex<Staging>,
    pub(crate) meshes: Mutex<ResourceAllocator<MeshResource, FRAMES_IN_FLIGHT>>,
    pub(crate) textures: Mutex<ResourceAllocator<TextureResource, FRAMES_IN_FLIGHT>>,
    pub(crate) shaders: Mutex<ResourceAllocator<ShaderResource, FRAMES_IN_FLIGHT>>,
    pub(crate) materials: Mutex<ResourceAllocator<MaterialResource, FRAMES_IN_FLIGHT>>,
    pub(crate) material_instances:
        Mutex<ResourceAllocator<MaterialInstanceResource, FRAMES_IN_FLIGHT>>,
    pub(crate) mesh_factory: Mutex<MeshFactory>,
    pub(crate) texture_factory: Mutex<TextureFactory>,
    pub(crate) material_factory: Mutex<MaterialFactory<FRAMES_IN_FLIGHT>>,
    ctx: Context,
}

impl Factory {
    pub(crate) fn new(ctx: Context, layouts: &Layouts) -> Self {
        let inner = Arc::new(FactoryInner {
            staging: Mutex::new(Staging::new(ctx.clone())),
            meshes: Mutex::new(ResourceAllocator::new(MAX_MESHES, DROP_LATENCY)),
            textures: Mutex::new(ResourceAllocator::new(MAX_TEXTURES, DROP_LATENCY)),
            shaders: Mutex::new(ResourceAllocator::new(MAX_SHADERS, DROP_LATENCY)),
            materials: Mutex::new(ResourceAllocator::new(MAX_MATERIALS, DROP_LATENCY)),
            material_instances: Mutex::new(ResourceAllocator::new(
                MAX_MATERIAL_INSTANCES,
                DROP_LATENCY,
            )),
            mesh_factory: Mutex::new(MeshFactory::new(
                ctx.clone(),
                layouts,
                // TODO: Load this from a config file
                MeshFactoryConfig {
                    base_vertex_block_len: 64,
                    base_index_block_len: 256,
                    base_meshlet_block_len: 8,
                    default_vertex_buffer_len: 65536,
                    default_index_buffer_len: 65536,
                    default_meshlet_buffer_len: 65536,
                },
                MAX_MESHES,
                FRAMES_IN_FLIGHT,
            )),
            texture_factory: Mutex::new(TextureFactory::new(&ctx, layouts, FRAMES_IN_FLIGHT)),
            material_factory: Mutex::new(MaterialFactory::new(
                ctx.clone(),
                layouts.materials.clone(),
                // TODO: Load this from a config file
                MaterialFactoryConfig {
                    default_materials_cap: HashMap::default(),
                    default_textures_cap: 128,
                    fallback_materials_cap: 32,
                },
            )),
            ctx: ctx.clone(),
        });

        // Primary passes
        ard_render_renderers::passes::define_passes(
            &mut inner.material_factory.lock().unwrap(),
            layouts,
        );

        // PBR setup
        let pbr_material = ard_render_pbr::create_pbr_material(
            ctx.properties(),
            |create_info| inner.create_shader(create_info).unwrap(),
            |create_info| inner.create_material(create_info).unwrap(),
        );

        Self {
            inner,
            pbr_material,
        }
    }

    pub(crate) fn process(&self, frame: Frame) {
        puffin::profile_function!();

        let mut staging = self.inner.staging.lock().unwrap();
        let mut static_meshes = self.inner.meshes.lock().unwrap();
        let mut textures = self.inner.textures.lock().unwrap();
        let mut shaders = self.inner.shaders.lock().unwrap();
        let mut materials = self.inner.materials.lock().unwrap();
        let mut material_instances = self.inner.material_instances.lock().unwrap();
        let mut mesh_factory = self.inner.mesh_factory.lock().unwrap();
        let mut texture_factory = self.inner.texture_factory.lock().unwrap();
        let mut material_factory = self.inner.material_factory.lock().unwrap();

        // Check for new upload requests
        staging.upload(&mut mesh_factory, &textures);

        // Check if any uploads are complete, and if they are, handle them appropriately
        staging.flush_complete_uploads(false, |resc| match resc {
            StagingResource::StaticMesh(id) => {
                // Flag mesh as being ready for rendering
                if let Some(mesh) = static_meshes.get_mut(id) {
                    mesh.ready = true;
                }
            }
            StagingResource::Texture(id) => texture_factory.texture_ready(id),
            StagingResource::TextureMip { id, mip_level } => {
                if let Some(texture) = textures.get_mut(id) {
                    // Update mip level
                    texture.loaded_mips |= 1 << mip_level;

                    // Tell the factory about the new mip
                    texture_factory.mip_update(MipUpdate::Texture(id));
                }
            }
        });

        // Flush modified material data and uploaded meshes
        material_factory.flush(
            frame,
            &material_instances,
            MATERIALS_SET_DATA_BINDING,
            MATERIALS_SET_TEXTURE_SLOTS_BINDING,
        );

        mesh_factory.flush_mesh_info(frame);
        mesh_factory.check_rebind(frame);

        // Drop pending resources
        static_meshes.drop_pending(
            frame,
            |_, mesh| {
                mesh_factory.free(mesh.block);
            },
            |_, _| {},
        );
        textures.drop_pending(
            frame,
            |_, _| {},
            |id, _| {
                texture_factory.texture_dropped(id);
            },
        );
        shaders.drop_pending(frame, |_, _| {}, |_, _| {});
        materials.drop_pending(frame, |_, _| {}, |_, _| {});
        material_instances.drop_pending(
            frame,
            |_, material| {
                if let Some(slot) = material.data_slot {
                    material_factory.free_data_slot(material.data.len() as u64, slot);
                }
                if let Some(slot) = material.textures_slot {
                    material_factory.free_textures_slot(slot);
                }
            },
            |_, _| {},
        );

        // Bind textures
        texture_factory.update_bindings(frame, &textures);
    }

    pub fn create_mesh<M: Into<MeshData>>(
        &self,
        create_info: MeshCreateInfo<M>,
    ) -> Result<Mesh, MeshCreateError> {
        self.inner.create_mesh(create_info)
    }

    pub fn create_texture<T: TextureSource>(
        &self,
        create_info: TextureCreateInfo<T>,
    ) -> Result<Texture, TextureCreateError<T>> {
        self.inner.create_texture(create_info)
    }

    pub fn create_shader(
        &self,
        create_info: ShaderCreateInfo,
    ) -> Result<Shader, ShaderCreateError> {
        self.inner.create_shader(create_info)
    }

    pub fn create_material(
        &self,
        create_info: MaterialCreateInfo,
    ) -> Result<Material, MaterialCreateError> {
        self.inner.create_material(create_info)
    }

    pub fn create_material_instance(
        &self,
        create_info: MaterialInstanceCreateInfo,
    ) -> Result<MaterialInstance, MaterialInstanceCreateError> {
        self.inner.create_material_instance(create_info)
    }

    pub fn create_pbr_material_instance(
        &self,
    ) -> Result<MaterialInstance, MaterialInstanceCreateError> {
        self.inner
            .create_material_instance(MaterialInstanceCreateInfo {
                material: self.pbr_material.clone(),
            })
    }

    pub fn load_texture_mip(&self, texture: &Texture, level: usize, source: impl TextureSource) {
        self.inner.load_texture_mip(texture, level, source)
    }

    pub fn set_material_data(
        &self,
        material_instance: &MaterialInstance,
        data: &(impl Pod + Zeroable),
    ) {
        self.inner.set_material_data(material_instance, data)
    }

    pub fn set_material_texture_slot(
        &self,
        material_instance: &MaterialInstance,
        slot: TextureSlot,
        texture: Option<&Texture>,
    ) {
        self.inner
            .set_material_texture_slot(material_instance, slot, texture)
    }
}

impl FactoryInner {
    fn create_mesh<M: Into<MeshData>>(
        &self,
        create_info: MeshCreateInfo<M>,
    ) -> Result<Mesh, MeshCreateError> {
        let mut staging = self.staging.lock().unwrap();
        let mut static_meshes = self.meshes.lock().unwrap();
        let mut mesh_factory = self.mesh_factory.lock().unwrap();

        // Create the mesh instance
        let (mesh, upload, info) = MeshResource::new(create_info, &self.ctx, &mut mesh_factory)?;

        // Create the resource handle
        let layout = mesh.block.layout();
        let handle = static_meshes.insert(mesh);

        // Upload info
        mesh_factory.set_mesh_info(handle.id(), info);

        // Submit the upload request
        staging.add(StagingRequest::Mesh {
            id: handle.id(),
            upload,
        });

        Ok(Mesh::new(handle, layout))
    }

    fn create_texture<T: TextureSource>(
        &self,
        create_info: TextureCreateInfo<T>,
    ) -> Result<Texture, TextureCreateError<T>> {
        let mut staging = self.staging.lock().unwrap();
        let mut textures = self.textures.lock().unwrap();

        // Create the texture instance
        let (texture, upload) = TextureResource::new(&self.ctx, create_info)?;

        // Create the resource handle
        let handle = textures.insert(texture);

        // Submit the upload request
        staging.add(StagingRequest::Texture {
            id: handle.id(),
            upload,
        });

        Ok(Texture::new(handle))
    }

    fn create_shader(&self, create_info: ShaderCreateInfo) -> Result<Shader, ShaderCreateError> {
        let mut shaders = self.shaders.lock().unwrap();

        let texture_slots = create_info.texture_slots;
        let data_size = create_info.data_size;
        let shader = ShaderResource::new(create_info, &self.ctx)?;

        let handle = shaders.insert(shader);

        Ok(Shader::new(handle, texture_slots, data_size))
    }

    fn create_material(
        &self,
        create_info: MaterialCreateInfo,
    ) -> Result<Material, MaterialCreateError> {
        let shaders = self.shaders.lock().unwrap();
        let mut materials = self.materials.lock().unwrap();
        let material_factory = self.material_factory.lock().unwrap();

        let data_size = create_info.data_size;
        let texture_slots = create_info.texture_slots;
        let material = MaterialResource::new(&self.ctx, &material_factory, &shaders, create_info)?;

        let handle = materials.insert(material);

        Ok(Material::new(handle, data_size, texture_slots))
    }

    fn create_material_instance(
        &self,
        create_info: MaterialInstanceCreateInfo,
    ) -> Result<MaterialInstance, MaterialInstanceCreateError> {
        let mut material_instances = self.material_instances.lock().unwrap();
        let mut material_factory = self.material_factory.lock().unwrap();

        let material = create_info.material.clone();
        let material_instance = MaterialInstanceResource::new(create_info, &mut material_factory)?;

        let data_slot = material_instance.data_slot;
        let tex_slot = material_instance.textures_slot;
        let handle = material_instances.insert(material_instance);

        Ok(MaterialInstance::new(handle, material, data_slot, tex_slot))
    }

    fn load_texture_mip(&self, texture: &Texture, level: usize, source: impl TextureSource) {
        let mut staging = self.staging.lock().unwrap();
        let textures = self.textures.lock().unwrap();

        let id = texture.id();
        let texture_inner = textures.get(id).unwrap();

        // Mip level must not already be loaded
        if texture_inner.loaded_mips & (1 << level) != 0 {
            return;
        }

        let data = source.into_texture_data().unwrap();

        let (width, height, _) = texture_inner.texture.dims();
        assert_eq!(data.width(), width >> level);
        assert_eq!(data.height(), height >> level);
        assert_eq!(data.format(), texture_inner.texture.format());

        let staging_buffer = Buffer::new_staging(
            self.ctx.clone(),
            QueueType::Transfer,
            Some(format!("texture_{id:?}_mip_level_{level}_staging")),
            data.raw(),
        )
        .unwrap();

        staging.add(StagingRequest::TextureMip {
            id,
            upload: TextureMipUpload {
                staging: staging_buffer,
                mip_level: level as u32,
            },
        });
    }

    fn set_material_data(
        &self,
        material_instance: &MaterialInstance,
        data: &(impl Pod + Zeroable),
    ) {
        let mut material_instances = self.material_instances.lock().unwrap();
        let mut material_factory = self.material_factory.lock().unwrap();

        let inner = material_instances.get_mut(material_instance.id()).unwrap();

        // Mark as dirty
        material_factory.mark_dirty(material_instance.clone());

        // Write in the data
        inner.data.copy_from_slice(bytemuck::bytes_of(data));
    }

    fn set_material_texture_slot(
        &self,
        material_instance: &MaterialInstance,
        slot: TextureSlot,
        texture: Option<&Texture>,
    ) {
        let mut material_instances = self.material_instances.lock().unwrap();
        let mut material_factory = self.material_factory.lock().unwrap();

        let inner = material_instances.get_mut(material_instance.id()).unwrap();
        inner.textures[slot.0 as usize] = texture.cloned();

        // Mark as dirty
        material_factory.mark_dirty(material_instance.clone());
    }
}
