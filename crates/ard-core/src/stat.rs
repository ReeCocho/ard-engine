use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use ard_ecs::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;
use fxhash::FxHashSet;

pub type StaticGroup = u32;

/// A component indicating that a particular entity is static. The definition of "static" is
/// dependent on how a system uses an entity. For example, for rendering and physics it might
/// mean that an entity does not move, which can allow for better optimizations. Assume that all
/// entities without this component are "dynamic".
///
/// The contained integer value is the static objects group identifier.
#[derive(Debug, Default, Component, Copy, Clone)]
pub struct Static(pub StaticGroup);

/// Resource used to mark certain static groups as being "dirty". This means that at least one of
/// the objects in the group have been modified, and systems that depend on those groups should be
/// notified.
#[derive(Debug, Resource, Clone)]
pub struct DirtyStatic(Arc<DirtyStaticInner>);

#[derive(Debug)]
struct DirtyStaticInner {
    /// Counter for unique listener IDs.
    listener_count: AtomicUsize,
    /// Listeners for particular groups.
    by_group: DashMap<StaticGroup, Vec<Listener>>,
    /// Listeners for all groups.
    all: DashMap<usize, Listener>,
}

#[derive(Debug)]
pub struct DirtyStaticListenerBuilder {
    src: DirtyStatic,
    all: bool,
    groups: FxHashSet<StaticGroup>,
}

/// Listener of a set of static groups that wishes to be notified when the entities in those groups
/// are modified.
#[derive(Debug)]
pub struct DirtyStaticListener {
    id: usize,
    src: DirtyStatic,
    all: bool,
    groups: FxHashSet<StaticGroup>,
    /// Receives incoming dirty signals from other signalers.
    recv: Receiver<StaticGroup>,
}

#[derive(Debug)]
struct Listener {
    id: usize,
    /// Used to signal to this listener.
    send: Sender<StaticGroup>,
}

impl Default for DirtyStatic {
    fn default() -> Self {
        Self(Arc::new(DirtyStaticInner {
            listener_count: AtomicUsize::new(0),
            by_group: DashMap::default(),
            all: DashMap::default(),
        }))
    }
}

impl DirtyStatic {
    pub fn listen(&self) -> DirtyStaticListenerBuilder {
        DirtyStaticListenerBuilder {
            src: self.clone(),
            all: false,
            groups: FxHashSet::default(),
        }
    }

    pub fn signal(&self, group: StaticGroup) {
        // Signal listeners of all groups
        for listener in self.0.all.iter() {
            let listener = listener.value();
            let _ = listener.send.send(group);
        }

        // Signal listeners for this specific group
        let listeners = match self.0.by_group.get(&group) {
            Some(listeners) => listeners,
            None => return,
        };

        for listener in listeners.iter() {
            let _ = listener.send.send(group);
        }
    }
}

impl DirtyStaticListenerBuilder {
    pub fn to_all(mut self) -> Self {
        self.all = true;
        self
    }

    pub fn to_group(mut self, group: StaticGroup) -> Self {
        self.groups.insert(group);
        self
    }

    pub fn build(mut self) -> DirtyStaticListener {
        if self.all {
            self.groups.clear();
        }

        let id = self.src.0.listener_count.fetch_add(1, Ordering::Relaxed);
        let (send, recv) = unbounded();

        if self.all {
            self.src.0.all.insert(id, Listener { id, send });
        } else {
            for group in &self.groups {
                let mut listeners = self.src.0.by_group.entry(*group).or_default();
                listeners.push(Listener {
                    id,
                    send: send.clone(),
                });
            }
        }

        DirtyStaticListener {
            id,
            src: self.src,
            all: self.all,
            groups: self.groups,
            recv,
        }
    }
}

impl DirtyStaticListener {
    /// Gets the next signal, or `None` if there was no signal.
    #[inline(always)]
    pub fn recv(&self) -> Option<StaticGroup> {
        self.recv.try_recv().ok()
    }
}

impl Drop for DirtyStaticListener {
    fn drop(&mut self) {
        // Remove from all list
        if self.all {
            self.src.0.all.remove(&self.id);
        }

        // Remove from groups list
        for group in &self.groups {
            let mut group = match self.src.0.by_group.get_mut(group) {
                Some(group) => group,
                None => continue,
            };

            for i in 0..group.len() {
                if group[i].id == self.id {
                    group.swap_remove(i);
                    break;
                }
            }
        }
    }
}
