use factory::{container::EscapeHandle, materials::MaterialBuffers};

use crate::prelude::*;

#[derive(Clone)]
pub struct Material {
    pub(crate) id: u32,
    pub(crate) pipeline_id: u32,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct MaterialInner {
    pub pipeline: Pipeline,
    pub material_slot: Option<u32>,
    pub texture_slot: Option<u32>,
    pub material_data: Vec<u8>,
    pub textures: Vec<Option<Texture>>,
}

impl MaterialInner {
    pub unsafe fn new(
        material_buffers: &mut MaterialBuffers,
        create_info: &MaterialCreateInfo<VkBackend>,
    ) -> MaterialInner {
        let material_slot = if create_info.pipeline.inputs.ubo_size > 0 {
            Some(material_buffers.allocate_ubo(create_info.pipeline.inputs.ubo_size))
        } else {
            None
        };

        let mut textures = Vec::with_capacity(create_info.pipeline.inputs.texture_count);
        textures.resize(create_info.pipeline.inputs.texture_count, None);

        let texture_slot = if create_info.pipeline.inputs.texture_count > 0 {
            Some(material_buffers.allocate_textures())
        } else {
            None
        };

        MaterialInner {
            pipeline: create_info.pipeline.clone(),
            material_data: vec![0; create_info.pipeline.inputs.ubo_size as usize],
            material_slot,
            texture_slot,
            textures,
        }
    }
}

impl MaterialApi for Material {}
