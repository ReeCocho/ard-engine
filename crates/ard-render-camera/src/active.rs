use std::collections::hash_map;

use ard_ecs::prelude::Entity;
use ard_render_objects::Model;
use rustc_hash::FxHashMap;

use crate::Camera;

#[derive(Default)]
pub struct ActiveCameras {
    cameras: FxHashMap<Entity, ActiveCamera>,
}

pub struct ActiveCamera {
    pub camera: Camera,
    pub model: Model,
}

impl ActiveCameras {
    pub fn main_camera(&self) -> Option<&ActiveCamera> {
        let mut depth = i32::MIN;
        let mut camera = None;

        for c in self.cameras.values() {
            if c.camera.order > depth {
                camera = Some(c);
                depth = c.camera.order;
            }
        }

        camera
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.cameras.clear();
    }

    #[inline(always)]
    pub fn insert(&mut self, entity: Entity, camera: ActiveCamera) {
        self.cameras.insert(entity, camera);
    }

    #[inline(always)]
    pub fn get(&self, entity: Entity) -> Option<&ActiveCamera> {
        self.cameras.get(&entity)
    }

    #[inline(always)]
    pub fn iter(&self) -> hash_map::Iter<'_, Entity, ActiveCamera> {
        self.cameras.iter()
    }
}
