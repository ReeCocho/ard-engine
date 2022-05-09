use crate::prelude::*;
use ard_ecs::prelude::*;

/// Used to create rendering resources for a renderer.
pub trait FactoryApi<B: Backend>: Resource + Clone + Send + Sync {
    fn create_mesh(&self, create_info: &MeshCreateInfo) -> B::Mesh;

    fn create_shader(&self, create_info: &ShaderCreateInfo) -> B::Shader;

    fn create_pipeline(&self, create_info: &PipelineCreateInfo<B>) -> B::Pipeline;

    fn create_material(&self, create_info: &MaterialCreateInfo<B>) -> B::Material;

    fn create_camera(&self, create_info: &CameraCreateInfo) -> B::Camera;

    fn create_texture(&self, create_info: &TextureCreateInfo) -> B::Texture;

    /// Gets the active main camera.
    ///
    /// # Note
    /// There is always a main camera. One is created automatically during renderer creation.
    fn main_camera(&self) -> B::Camera;

    fn update_camera(&self, camera: &B::Camera, descriptor: CameraDescriptor);

    fn update_material_data(&self, material: &B::Material, data: &[u8]);

    fn update_material_texture(&self, material: &B::Material, texture: &B::Texture, slot: usize);

    /// Load a particular mip level for a texture created with `MipType::UploadLater`.
    fn load_texture_mip(&self, texture: &B::Texture, level: usize, data: &[u8]);
}
