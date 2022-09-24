use std::{collections::HashMap, sync::Mutex};

use ard_ecs::prelude::*;
use ard_math::Mat4;
use crossbeam_channel::{Receiver, Sender};

use crate::{
    renderer::{
        render_data::{make_draw_key, DrawKey},
        Model, Renderable,
    },
    shader_constants::FRAMES_IN_FLIGHT,
};

#[derive(Resource)]
pub struct StaticGeometry(pub(crate) Mutex<StaticGeometryInner>);

pub struct StaticRenderable {
    /// The description of the object to render.
    pub renderable: Renderable,
    /// Where the object is located in space.
    pub model: Model,
    /// The entity associated with the object.
    pub entity: Entity,
}

pub struct StaticRenderableHandle {
    id: u32,
    on_drop: Sender<u32>,
}

pub(crate) struct StaticGeometryInner {
    /// All static batches to render.
    pub batches: HashMap<DrawKey, StaticBatch>,
    /// Every draw key in batches, but sorted for optimal iteration.
    pub sorted_keys: Vec<DrawKey>,
    /// The total number of static objects.
    pub len: usize,
    /// Marks a certain frame as having been modified and thus needing a reupload.
    pub dirty: [bool; FRAMES_IN_FLIGHT],
    /// Maps the ID of a static object to the batch and index it exists in.
    id_to_batch: HashMap<u32, (DrawKey, usize)>,
    /// ID counter for static objects.
    id_counter: u32,
    /// Dropped static objects.
    dropped: Receiver<u32>,
    /// For object handles to signal a drop.
    on_drop: Sender<u32>,
}

pub(crate) struct StaticBatch {
    pub renderable: Renderable,
    pub models: Vec<Mat4>,
    pub entities: Vec<Entity>,
    pub ids: Vec<u32>,
}

impl Default for StaticGeometry {
    fn default() -> Self {
        let (on_drop, dropped) = crossbeam_channel::unbounded();
        StaticGeometry(Mutex::new(StaticGeometryInner {
            batches: HashMap::default(),
            sorted_keys: Vec::default(),
            len: 0,
            id_to_batch: HashMap::default(),
            dirty: [false; FRAMES_IN_FLIGHT],
            id_counter: 0,
            dropped,
            on_drop,
        }))
    }
}

impl StaticGeometry {
    /// Registers new static objects to be rendered.
    ///
    /// # Note on Performance
    /// You should try to minimize the number of times this function is called. Calling it forces
    /// the renderer to reupload all static geometry. Ideally, batch together calls to this
    /// function.
    pub fn register(&self, renderables: &[StaticRenderable]) -> Vec<StaticRenderableHandle> {
        let mut inner = self.0.lock().unwrap();
        let mut inner = &mut *inner;

        let mut handles = Vec::with_capacity(renderables.len());

        // Mark every frame as having dirty geometry
        for flag in &mut inner.dirty {
            *flag = true;
        }

        // Update object count
        inner.len += renderables.len();

        // Register
        for (i, renderable) in renderables.iter().enumerate() {
            // Generate an ID for the renderable
            let key = make_draw_key(&renderable.renderable.material, &renderable.renderable.mesh);
            let id = inner.id_counter;
            inner.id_counter += 1;

            // Find the batch to put the renderable into
            let batch = inner.batches.entry(key).or_insert_with(|| {
                // Batch doesn't exist. Add sorted key and insert new batch
                if let Err(pos) = inner.sorted_keys.binary_search(&key) {
                    inner.sorted_keys.insert(pos, key);
                }

                StaticBatch {
                    renderable: renderable.renderable.clone(),
                    models: Vec::default(),
                    entities: Vec::default(),
                    ids: Vec::default(),
                }
            });

            // Add the renderable to the batch
            batch.models.push(renderable.model.0);
            batch.entities.push(renderable.entity);
            batch.ids.push(id);
            inner.id_to_batch.insert(id, (key, batch.ids.len() - 1));

            // Output the handle
            handles.push(StaticRenderableHandle {
                id,
                on_drop: inner.on_drop.clone(),
            });
        }

        handles
    }
}

impl StaticGeometryInner {
    /// Cleans up dropped objects.
    pub fn cleanup(&mut self) {
        let mut dirty = false;

        while let Ok(id) = self.dropped.try_recv() {
            dirty = true;
            let (key, idx) = self.id_to_batch.remove(&id).unwrap();
            let batch = self.batches.get_mut(&key).unwrap();

            // Remove object
            self.len -= 1;
            batch.models.swap_remove(idx);
            batch.entities.swap_remove(idx);
            batch.ids.swap_remove(idx);

            // Update index of swapped object
            if idx != batch.ids.len() {
                let swapped_id = batch.ids[idx];
                self.id_to_batch.insert(swapped_id, (key, idx));
            }

            // Remove the batch if it is empty
            if batch.ids.is_empty() {
                self.batches.remove(&key);
            }
        }

        if dirty {
            for flag in &mut self.dirty {
                *flag = true;
            }
        }
    }
}

impl Drop for StaticRenderableHandle {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = self.on_drop.send(self.id);
    }
}
