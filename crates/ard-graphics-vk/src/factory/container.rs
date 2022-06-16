use crate::shader_constants::FRAMES_IN_FLIGHT;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;

/// Container type for renderer resources.
pub(crate) struct ResourceContainer<T> {
    pub free_ids: Vec<u32>,
    pub resources: Vec<Option<T>>,
    pub sender: Sender<u32>,
    pub receiver: Receiver<u32>,
    /// ## Note
    /// We maintain two lists for each frame because we need two frames of latency between when a
    /// resource is requested to be destroyed and when it is actually destroyed. This is because
    /// the occlusion culling algorithm needs to redraw static objects from the previous frame, so
    /// we need to ensure those resources remain in memory.
    pub pending_drop: [[Vec<u32>; 2]; FRAMES_IN_FLIGHT],
}

/// Handle to an object in a resource container that, when dropped, signals the resource should
/// be destroyed.
#[derive(Clone)]
pub(crate) struct EscapeHandle(Arc<EscapeHandleInner>);

struct EscapeHandleInner {
    drop_sender: Sender<u32>,
    id: u32,
}

impl<T> ResourceContainer<T> {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            free_ids: Vec::default(),
            resources: Vec::default(),
            sender,
            receiver,
            pending_drop: Default::default(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.resources.len() - self.free_ids.len()
    }

    /// Drops pending resources for the provided frame and puts newly dropped resources in the
    /// container for next time. Runs a closure for each dropped resource when they are dropped
    /// and also when they are first found.
    pub fn drop_pending(
        &mut self,
        frame: usize,
        on_drop: &mut impl FnMut(u32, &mut T),
        on_found: &mut impl FnMut(u32, &mut T),
    ) {
        // Drop everything that needs dropping now
        for id in self.pending_drop[frame][0].drain(..) {
            on_drop(
                id,
                self.resources[id as usize]
                    .as_mut()
                    .expect("invalid resource id"),
            );
            self.resources[id as usize] = None;
            self.free_ids.push(id);
        }

        // Move things that need to be dropped now into the "now" spot
        self.pending_drop[frame].reverse();

        // Move things that are just now being dropped to the "later" spot
        for id in self.receiver.try_iter() {
            on_found(
                id,
                self.resources[id as usize]
                    .as_mut()
                    .expect("invalid resource id"),
            );
            self.pending_drop[frame][1].push(id);
        }
    }

    pub fn insert(&mut self, resource: T) -> EscapeHandle {
        let id = if let Some(id) = self.free_ids.pop() {
            self.resources[id as usize] = Some(resource);
            id
        } else {
            self.resources.push(Some(resource));
            self.resources.len() as u32 - 1
        };

        EscapeHandle(Arc::new(EscapeHandleInner {
            drop_sender: self.sender.clone(),
            id,
        }))
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.resources[id as usize].as_ref()
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut T> {
        self.resources[id as usize].as_mut()
    }
}

impl EscapeHandle {
    #[inline]
    pub fn id(&self) -> u32 {
        self.0.id
    }
}

impl Drop for EscapeHandleInner {
    fn drop(&mut self) {
        let _ = self.drop_sender.send(self.id);
    }
}
