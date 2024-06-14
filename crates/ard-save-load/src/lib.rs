use ard_assets::manager::Assets;
use entity_map::EntityMap;
use serde::{de::DeserializeOwned, Serialize};

pub mod entity_map;
pub mod load_data;
pub mod loader;
pub mod save_data;
pub mod saver;

pub trait SaveLoad {
    type Intermediate: Serialize + DeserializeOwned;

    fn save(&self, ctx: &SaveContext) -> Self::Intermediate;

    fn load(ctx: &LoadContext, intermediate: Self::Intermediate) -> Self;
}

impl<T: Serialize + DeserializeOwned + Clone> SaveLoad for T {
    type Intermediate = Self;

    #[inline(always)]
    fn save(&self, _: &SaveContext) -> Self::Intermediate {
        self.clone()
    }

    #[inline(always)]
    fn load(_: &LoadContext, intermediate: Self::Intermediate) -> Self {
        intermediate
    }
}

pub struct SaveContext {
    pub entity_map: EntityMap,
    pub assets: Assets,
}

pub struct LoadContext {
    pub entity_map: EntityMap,
    pub assets: Assets,
}
