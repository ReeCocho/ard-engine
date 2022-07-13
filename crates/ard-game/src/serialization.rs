use ard_assets::manager::Assets;
use ard_ecs::prelude::Component;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::scene::EntityMap;

#[derive(Serialize, Deserialize)]
pub enum ObjectDescriptor {
    // StaticObject
}

pub trait SerializableComponent: Sized + Component {
    type Descriptor: Serialize + DeserializeOwned;

    /// Serializes the component using it's descriptor object.
    fn save(&self, entities: &EntityMap, assets: &Assets) -> Self::Descriptor;

    /// Converts a sequence of descriptors into components.
    fn load(
        descriptors: Vec<Self::Descriptor>,
        entities: &EntityMap,
        assets: &Assets,
    ) -> Result<Vec<Self>, ()>;
}

impl<T: Serialize + DeserializeOwned + Clone + Component> SerializableComponent for T {
    type Descriptor = Self;

    #[inline]
    fn save(&self, _: &EntityMap, _: &Assets) -> Self {
        self.clone()
    }

    #[inline]
    fn load(
        descriptors: Vec<Self::Descriptor>,
        _: &EntityMap,
        _: &Assets,
    ) -> Result<Vec<Self>, ()> {
        Ok(descriptors)
    }
}
