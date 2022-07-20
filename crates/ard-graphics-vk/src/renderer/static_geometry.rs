use std::{
    hash::BuildHasherDefault,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex, RwLock, RwLockWriteGuard,
    },
};

use crate::{
    prelude::*,
    shader_constants::FRAMES_IN_FLIGHT,
    util::{make_draw_key, FastIntHasher},
};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::Mat4;
use dashmap::DashMap;

use super::forward_plus::DrawKey;

#[derive(Resource, Clone)]
pub struct StaticGeometry(pub(crate) Arc<StaticGeometryInner>);

pub(crate) struct StaticGeometryInner {
    pub(crate) batches: DashMap<DrawKey, StaticBatch, BuildHasherDefault<FastIntHasher>>,
    pub(crate) sorted_keys: RwLock<Vec<DrawKey>>,
    /// Indicates, for the specific frame, that the static geometry has been changed and needs to
    /// be reuploaded.
    pub(crate) dirty: [AtomicBool; FRAMES_IN_FLIGHT],
    pub(crate) len: AtomicUsize,
    handle_to_object:
        DashMap<StaticRenderableHandle, (DrawKey, usize), BuildHasherDefault<FastIntHasher>>,
    handle_ctr: AtomicU32,
    /// Used by the renderer to acquire exclusive access to static geometry.
    exclusive: RwLock<()>,
}

pub(crate) struct StaticBatch {
    #[allow(unused)]
    pub material: Material,
    pub mesh: Mesh,
    pub layers: RenderLayerFlags,
    pub models: Vec<Mat4>,
    pub entities: Vec<Entity>,
    pub handles: Vec<StaticRenderableHandle>,
}

impl StaticGeometry {
    pub(crate) fn new() -> Self {
        StaticGeometry(Arc::new(StaticGeometryInner {
            batches: DashMap::default(),
            sorted_keys: RwLock::default(),
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
        renderables: &[StaticRenderable<VkBackend>],
        handles: &mut [StaticRenderableHandle],
    ) {
        let _lock = self.0.exclusive.read().expect("lock poisoned");

        for flag in &self.0.dirty {
            flag.store(true, Ordering::Relaxed);
        }

        let batches = &self.0.batches;
        let handle_ctr = &self.0.handle_ctr;
        for (i, renderable) in renderables.iter().enumerate() {
            let key = make_draw_key(&renderable.renderable.material, &renderable.renderable.mesh);
            let handle = StaticRenderableHandle::new(handle_ctr.fetch_add(1, Ordering::Relaxed));

            let mut batch = if batches.contains_key(&key) {
                // Batch already exists
                batches.get_mut(&key).unwrap()
            } else {
                // Batch doesn't exist. Add sorted key and insert new batch
                let mut sorted_keys = self.0.sorted_keys.write().expect("mutex poisoned");
                if let Err(pos) = sorted_keys.binary_search(&key) {
                    sorted_keys.insert(pos, key);
                }

                batches.entry(key).or_insert(StaticBatch {
                    material: renderable.renderable.material.clone(),
                    mesh: renderable.renderable.mesh.clone(),
                    layers: renderable.renderable.layers,
                    models: Vec::default(),
                    entities: Vec::default(),
                    handles: Vec::default(),
                })
            };

            batch.models.push(renderable.model.0);
            batch.entities.push(renderable.entity);
            batch.handles.push(handle);

            if i < handles.len() {
                handles[i] = handle;
            }
        }

        self.0.len.fetch_add(renderables.len(), Ordering::Relaxed);
    }

    fn unregister(&self, handles: &[StaticRenderableHandle]) {
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
            batch.entities.swap_remove(idx);
            batch.handles.swap_remove(idx);

            if idx != batch.handles.len() {
                let handle_to_update = batch.handles[idx];
                handle_to_obj.insert(handle_to_update, (key, idx));
            }
        }

        self.0.len.fetch_sub(handles.len(), Ordering::Relaxed);
    }
}
