use ard_assets::prelude::{AssetNameBuf, Handle};
use ard_ecs::prelude::*;
use ard_graphics_assets::prelude::ModelAsset;
use serde::{Deserialize, Serialize};

use crate::serialization::SerializableComponent;

#[derive(Default, Component)]
pub struct RenderableData {
    pub source: Option<RenderableSource>,
}

pub enum RenderableSource {
    Model {
        model: Handle<ModelAsset>,
        mesh_group_idx: usize,
        mesh_idx: usize,
    },
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RenderableDataDescriptor {
    pub source: Option<RenderableSourceDescriptor>,
}

#[derive(Serialize, Deserialize)]
pub enum RenderableSourceDescriptor {
    Model {
        model: AssetNameBuf,
        mesh_group_idx: usize,
        mesh_idx: usize,
    },
}

impl SerializableComponent for RenderableData {
    type Descriptor = RenderableDataDescriptor;

    fn save(
        &self,
        _: &crate::scene::EntityMap,
        assets: &ard_assets::manager::Assets,
    ) -> Self::Descriptor {
        let mut descriptor = RenderableDataDescriptor::default();

        if let Some(source) = &self.source {
            descriptor.source = Some(match source {
                RenderableSource::Model {
                    model,
                    mesh_group_idx,
                    mesh_idx,
                } => RenderableSourceDescriptor::Model {
                    model: assets.get_name(model),
                    mesh_group_idx: *mesh_group_idx,
                    mesh_idx: *mesh_idx,
                },
            });
        }

        descriptor
    }

    fn load(
        descriptors: Vec<Self::Descriptor>,
        _: &crate::scene::EntityMap,
        assets: &ard_assets::manager::Assets,
    ) -> Result<Vec<Self>, ()> {
        let mut data = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            let mut rdata = RenderableData::default();
            if let Some(source) = descriptor.source {
                rdata.source = Some(match source {
                    RenderableSourceDescriptor::Model {
                        model,
                        mesh_group_idx,
                        mesh_idx,
                    } => RenderableSource::Model {
                        model: assets.load(&model),
                        mesh_group_idx,
                        mesh_idx,
                    },
                });
            }
            data.push(rdata);
        }
        Ok(data)
    }
}
