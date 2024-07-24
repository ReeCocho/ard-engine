use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::{Frame, FRAMES_IN_FLIGHT};

pub struct ResourceAllocator<R> {
    max: usize,
    ignore_drop: bool,
    resources: Vec<Resource<R>>,
    free: Vec<ResourceId>,
    dropped: Receiver<ResourceId>,
    on_drop: Sender<ResourceId>,
    /// First dim is for frames in flight. Second dim is for latency. Third is for the resources
    /// to drop.
    pending_drop: [Vec<Vec<ResourceId>>; FRAMES_IN_FLIGHT],
}

pub struct Resource<R> {
    pub resource: Option<R>,
    version: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(usize);

#[derive(Clone)]
pub struct ResourceHandle(Arc<ResourceHandleInner>);

struct ResourceHandleInner {
    on_drop: Sender<ResourceId>,
    id: ResourceId,
}

impl<R> ResourceAllocator<R> {
    /// Creates a resource allocator for a paticular resources.
    ///
    /// `max` is the maximum number of resources that can be allocated from this allocator.
    ///
    /// `latency` is the number of calls to `drop_pending` must be performed before a resource
    /// is actually destroyed.
    pub fn new(max: usize, latency: usize, ignore_drop: bool) -> Self {
        let (on_drop, dropped) = crossbeam_channel::bounded(max);
        let resources = Vec::with_capacity(max);

        let pending_drop = std::array::from_fn(|_| (0..latency).map(|_| Vec::default()).collect());

        Self {
            max,
            resources,
            ignore_drop,
            free: Vec::default(),
            on_drop,
            dropped,
            pending_drop,
        }
    }

    pub fn drop_pending(
        &mut self,
        frame: Frame,
        mut on_drop: impl FnMut(ResourceId, R),
        mut on_found: impl FnMut(ResourceId, &mut R),
        mut should_drop: impl FnMut(ResourceId, &R) -> bool,
    ) {
        if self.ignore_drop {
            return;
        }

        let frame = usize::from(frame);

        // Drop everything that needs dropping now
        self.pending_drop[frame][0].retain_mut(|id| {
            let resc = self.resources.get_mut(id.0).unwrap();
            let drop = should_drop(*id, resc.resource.as_ref().unwrap());
            if drop {
                on_drop(*id, resc.resource.take().unwrap());
                resc.version += 1;
                self.free.push(*id);
            }
            !drop
        });

        // Cycle every one of the "latency" lists
        let mut latency_lists = std::mem::take(&mut self.pending_drop[frame]);
        let latency = latency_lists.len();
        latency_lists = latency_lists
            .into_iter()
            .cycle()
            .skip(1)
            .take(latency)
            .collect();
        self.pending_drop[frame] = latency_lists;

        // Move things that are just now being dropped to the "later" spot
        let last_list = self.pending_drop[frame].last_mut().unwrap();
        for id in self.dropped.try_iter() {
            on_found(id, self.resources[id.0].resource.as_mut().unwrap());
            last_list.push(id);
        }
    }

    #[inline(always)]
    pub fn all(&self) -> &[Resource<R>] {
        &self.resources
    }

    #[inline(always)]
    pub fn allocated(&self) -> usize {
        self.resources.len() - self.free.len()
    }

    pub fn insert(&mut self, resource: R) -> ResourceHandle {
        let id = match self.free.pop() {
            Some(id) => {
                let resc = &mut self.resources[id.0];
                resc.resource = Some(resource);
                id
            }
            None => {
                assert_ne!(self.resources.len(), self.max, "max number of allocations");
                self.resources.push(Resource::<R> {
                    resource: Some(resource),
                    version: 0,
                });
                ResourceId(self.resources.len() - 1)
            }
        };
        ResourceHandle(Arc::new(ResourceHandleInner {
            on_drop: self.on_drop.clone(),
            id,
        }))
    }

    #[inline(always)]
    pub fn version_of(&self, id: ResourceId) -> Option<u32> {
        self.resources.get(id.0).map(|r| r.version)
    }

    #[inline(always)]
    pub fn get(&self, id: ResourceId) -> Option<&R> {
        self.resources.get(id.0).and_then(|r| r.resource.as_ref())
    }

    #[inline(always)]
    pub fn get_mut(&mut self, id: ResourceId) -> Option<&mut R> {
        self.resources
            .get_mut(id.0)
            .and_then(|r| r.resource.as_mut())
    }
}

impl ResourceHandle {
    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.0.id
    }
}

impl Drop for ResourceHandleInner {
    fn drop(&mut self) {
        let _ = self.on_drop.send(self.id);
    }
}

impl From<ResourceId> for usize {
    #[inline(always)]
    fn from(id: ResourceId) -> Self {
        id.0 as usize
    }
}

impl From<ResourceId> for u64 {
    fn from(value: ResourceId) -> Self {
        value.0 as u64
    }
}

impl From<usize> for ResourceId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<u64> for ResourceId {
    fn from(value: u64) -> Self {
        Self(value as usize)
    }
}
