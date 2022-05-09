use std::{
    hash::BuildHasherDefault,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex, RwLock, RwLockWriteGuard,
    },
};

use crate::{
    prelude::*,
    util::{make_draw_key, FastIntHasher},
};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::Mat4;
use dashmap::DashMap;

use super::{forward_plus::DrawKey, graph::FRAMES_IN_FLIGHT};

#[derive(Resource, Clone)]
pub struct StaticGeometry(pub(crate) Arc<StaticGeometryInner>);

pub(crate) struct StaticGeometryInner {
    pub(crate) batches: DashMap<DrawKey, StaticBatch, BuildHasherDefault<FastIntHasher>>,
    pub(crate) sorted_keys: Mutex<Vec<DrawKey>>,
    /// Indicates, for the specific frame, that the static geometry has been changed and needs to
    /// be reuploaded.
    pub(crate) dirty: [AtomicBool; FRAMES_IN_FLIGHT],
    pub(crate) len: AtomicUsize,
    handle_to_object:
        DashMap<StaticRenderable, (DrawKey, usize), BuildHasherDefault<FastIntHasher>>,
    handle_ctr: AtomicU32,
    /// Used by the renderer to acquire exclusive access to static geometry.
    exclusive: RwLock<()>,
}

pub(crate) struct StaticBatch {
    #[allow(unused)]
    pub material: Material,
    pub mesh: Mesh,
    pub models: Vec<Mat4>,
    pub handles: Vec<StaticRenderable>,
}

impl StaticGeometry {
    pub(crate) fn new() -> Self {
        StaticGeometry(Arc::new(StaticGeometryInner {
            batches: DashMap::default(),
            sorted_keys: Mutex::default(),
            handle_to_object: DashMap::default(),
            handle_ctr: AtomicU32::default(),
            len: AtomicUsize::default(),
            dirty: Default::default(),
            exclusive: RwLock::default(),
        }))
    }

    pub(crate) fn acquire(&self) -> RwLockWriteGuard<()> {
        self.0.exclusive.write().expect("mutex poisoned")
    }
}

impl StaticGeometryApi<VkBackend> for StaticGeometry {
    fn register(
        &self,
        models: &[(Renderable<VkBackend>, Model)],
        handles: &mut [StaticRenderable],
    ) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");

        for flag in &self.0.dirty {
            flag.store(true, Ordering::Relaxed);
        }

        let batches = &self.0.batches;
        let handle_ctr = &self.0.handle_ctr;
        for (i, model) in models.iter().enumerate() {
            let key = make_draw_key(&model.0.material, &model.0.mesh);
            let handle = StaticRenderable::new(handle_ctr.fetch_add(1, Ordering::Relaxed));

            let mut batch = if batches.contains_key(&key) {
                // Batch already exists
                batches.get_mut(&key).unwrap()
            } else {
                // Batch doesn't exist. Add sorted key and insert new batch
                let mut sorted_keys = self.0.sorted_keys.lock().expect("mutex poisoned");
                if let Err(pos) = sorted_keys.binary_search(&key) {
                    sorted_keys.insert(pos, key);
                }

                batches.entry(key).or_insert(StaticBatch {
                    material: model.0.material.clone(),
                    mesh: model.0.mesh.clone(),
                    models: Vec::default(),
                    handles: Vec::default(),
                })
            };

            batch.models.push(model.1 .0);
            batch.handles.push(handle);

            if i < handles.len() {
                handles[i] = handle;
            }
        }

        self.0.len.fetch_add(models.len(), Ordering::Relaxed);
    }

    fn unregister(&self, handles: &[StaticRenderable]) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");

        for flag in &self.0.dirty {
            flag.store(true, Ordering::Relaxed);
        }

        let batches = &self.0.batches;
        let handle_to_obj = &self.0.handle_to_object;
        for handle in handles {
            let (key, idx) = *handle_to_obj.get(handle).expect("double free");
            let mut batch = batches.get_mut(&key).unwrap();
            batch.models.swap_remove(idx);
            batch.handles.swap_remove(idx);

            if idx != batch.handles.len() {
                let handle_to_update = batch.handles[idx];
                handle_to_obj.insert(handle_to_update, (key, idx));
            }
        }

        self.0.len.fetch_sub(handles.len(), Ordering::Relaxed);
    }
}
