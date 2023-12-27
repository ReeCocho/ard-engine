use ard_ecs::prelude::Component;
use ard_render_base::resource::{ResourceHandle, ResourceId};
use ard_render_textures::texture::Texture;
use thiserror::Error;

use crate::{
    factory::{buffer::MaterialSlot, MaterialFactory},
    material::Material,
};

pub struct MaterialInstanceCreateInfo {
    /// Material we are going to make an instance of.
    pub material: Material,
}

#[derive(Debug, Error)]
#[error("unknown")]
pub struct MaterialInstanceCreateError;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureSlot(pub u32);

/// Describes the surface properties of an object.
#[derive(Clone, Component)]
pub struct MaterialInstance {
    material: Material,
    data_slot: Option<MaterialSlot>,
    tex_slot: Option<MaterialSlot>,
    handle: ResourceHandle,
}

pub struct MaterialInstanceResource {
    pub material: Material,
    pub data: Vec<u8>,
    pub textures: Vec<Option<Texture>>,
    pub data_slot: Option<MaterialSlot>,
    pub textures_slot: Option<MaterialSlot>,
}

impl MaterialInstance {
    pub fn new(
        handle: ResourceHandle,
        material: Material,
        data_slot: Option<MaterialSlot>,
        texture_slot: Option<MaterialSlot>,
    ) -> MaterialInstance {
        MaterialInstance {
            handle,
            material,
            data_slot,
            tex_slot: texture_slot,
        }
    }

    #[inline(always)]
    pub fn data_slot(&self) -> Option<MaterialSlot> {
        self.data_slot
    }

    #[inline(always)]
    pub fn tex_slot(&self) -> Option<MaterialSlot> {
        self.tex_slot
    }

    #[inline(always)]
    pub fn material(&self) -> &Material {
        &self.material
    }

    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.handle.id()
    }
}

impl MaterialInstanceResource {
    pub fn new<const FRAMES_IN_FLIGHT: usize>(
        create_info: MaterialInstanceCreateInfo,
        factory: &mut MaterialFactory<FRAMES_IN_FLIGHT>,
    ) -> Result<Self, MaterialInstanceCreateError> {
        factory.verify_set(create_info.material.data_size() as u64);

        let data_slot = if create_info.material.data_size() > 0 {
            Some(factory.allocate_data_slot(create_info.material.data_size() as u64))
        } else {
            None
        };

        let textures_slot = if create_info.material.texture_slots() > 0 {
            Some(factory.allocate_textures_slot())
        } else {
            None
        };

        Ok(MaterialInstanceResource {
            data: vec![0; create_info.material.data_size() as usize],
            textures: vec![None; create_info.material.texture_slots() as usize],
            material: create_info.material,
            data_slot,
            textures_slot,
        })
    }
}

impl TextureSlot {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self(ard_render_si::consts::EMPTY_TEXTURE_ID)
    }
}
