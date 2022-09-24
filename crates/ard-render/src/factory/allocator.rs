use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::shader_constants::FRAMES_IN_FLIGHT;

pub(crate) struct ResourceAllocator<T> {
    max: usize,
    resources: Vec<Option<T>>,
    free: Vec<ResourceId>,
    dropped: Receiver<ResourceId>,
    on_drop: Sender<ResourceId>,
    /// ## Note
    /// We maintain two lists for each frame because we need two frames of latency between when a
    /// resource is requested to be destroyed and when it is actually destroyed. This is because
    /// the occlusion culling algorithm needs to redraw static objects from the previous frame, so
    /// we need to ensure those resources remain in memory.
    pending_drop: [[Vec<ResourceId>; 2]; FRAMES_IN_FLIGHT],
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResourceId(pub(crate) usize);

#[derive(Clone)]
pub(crate) struct EscapeHandle(Arc<EscapeHandleInner>);

struct EscapeHandleInner {
    on_drop: Sender<ResourceId>,
    id: ResourceId,
}

impl<T> ResourceAllocator<T> {
    pub fn new(max: usize) -> Self {
        let (on_drop, dropped) = crossbeam_channel::bounded(max);
        let resources = Vec::with_capacity(max);
        Self {
            max,
            resources,
            free: Vec::default(),
            on_drop,
            dropped,
            pending_drop: Default::default(),
        }
    }

    #[inline(always)]
    pub fn all(&self) -> &[Option<T>] {
        &self.resources
    }

    #[inline(always)]
    pub fn all_mut(&mut self) -> &mut [Option<T>] {
        &mut self.resources
    }

    pub fn drop_pending(
        &mut self,
        frame: usize,
        mut on_drop: impl FnMut(ResourceId, &mut T),
        mut on_found: impl FnMut(ResourceId, &mut T),
    ) {
        // Drop everything that needs dropping now
        for id in self.pending_drop[frame][0].drain(..) {
            on_drop(
                id,
                self.resources[id.0].as_mut().expect("invalid resource id"),
            );
            self.resources[id.0] = None;
            self.free.push(id);
        }

        // Move things that need to be dropped next into the "now" spot
        self.pending_drop[frame].reverse();

        // Move things that are just now being dropped to the "later" spot
        for id in self.dropped.try_iter() {
            on_found(
                id,
                self.resources[id.0].as_mut().expect("invalid resource id"),
            );
            self.pending_drop[frame][1].push(id);
        }
    }

    pub fn insert(&mut self, resource: T) -> EscapeHandle {
        let id = match self.free.pop() {
            Some(id) => {
                self.resources[id.0] = Some(resource);
                id
            }
            None => {
                assert_ne!(self.resources.len(), self.max, "max number of allocations");
                self.resources.push(Some(resource));
                ResourceId(self.resources.len() - 1)
            }
        };
        EscapeHandle(Arc::new(EscapeHandleInner {
            on_drop: self.on_drop.clone(),
            id,
        }))
    }

    #[inline(always)]
    pub fn get(&self, id: ResourceId) -> Option<&T> {
        self.resources[id.0].as_ref()
    }

    #[inline(always)]
    pub fn get_mut(&mut self, id: ResourceId) -> Option<&mut T> {
        self.resources[id.0].as_mut()
    }
}

impl EscapeHandle {
    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.0.id
    }
}

impl Drop for EscapeHandleInner {
    fn drop(&mut self) {
        let _ = self.on_drop.send(self.id);
    }
}

impl From<ResourceId> for usize {
    #[inline(always)]
    fn from(id: ResourceId) -> Self {
        id.0
    }
}
