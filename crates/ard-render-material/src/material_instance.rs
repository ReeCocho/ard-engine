use std::sync::Arc;

use ard_ecs::prelude::Component;
use ard_render_base::{
    resource::{ResourceHandle, ResourceId},
    FRAMES_IN_FLIGHT,
};
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
pub struct TextureSlot(pub u16);

/// Describes the surface properties of an object.
#[derive(Clone, Component)]
pub struct MaterialInstance {
    material: Material,
    data_ptrs: Option<Arc<[u64; FRAMES_IN_FLIGHT]>>,
    tex_slot: Option<MaterialSlot>,
    handle: ResourceHandle,
}

pub struct MaterialInstanceResource {
    pub material: Material,
    pub data: Vec<u8>,
    pub textures: Vec<Option<Texture>>,
    pub data_slot: Option<MaterialSlot>,
    pub data_ptrs: Option<Arc<[u64; FRAMES_IN_FLIGHT]>>,
    pub textures_slot: Option<MaterialSlot>,
}

impl MaterialInstance {
    pub fn new(
        handle: ResourceHandle,
        material: Material,
        data_ptrs: Option<Arc<[u64; FRAMES_IN_FLIGHT]>>,
        tex_slot: Option<MaterialSlot>,
    ) -> MaterialInstance {
        MaterialInstance {
            handle,
            material,
            data_ptrs,
            tex_slot,
        }
    }

    #[inline(always)]
    pub fn data_ptrs(&self) -> Option<&Arc<[u64; FRAMES_IN_FLIGHT]>> {
        self.data_ptrs.as_ref()
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
    pub fn new(
        create_info: MaterialInstanceCreateInfo,
        factory: &mut MaterialFactory,
    ) -> Result<Self, MaterialInstanceCreateError> {
        let (data_slot, data_ptrs) = if create_info.material.data_size() > 0 {
            // Allocate the slot
            let slot = factory.allocate_data_slot(create_info.material.data_size() as u64);

            // Find data pointers
            // NOTE: Safe to unwrap since we just allocated a slot in it
            let buffer = factory
                .get_material_buffer(create_info.material.data_size() as u64)
                .unwrap();
            let ptrs = std::array::from_fn(|frame| {
                buffer.buffer().device_ref(frame)
                    + u64::from(slot) * create_info.material.data_size() as u64
            });

            (Some(slot), Some(Arc::new(ptrs)))
        } else {
            (None, None)
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
            data_ptrs,
            textures_slot,
        })
    }
}

impl TextureSlot {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self(ard_render_si::consts::EMPTY_TEXTURE_ID as u16)
    }
}
