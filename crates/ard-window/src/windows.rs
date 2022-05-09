use std::collections::HashMap;

use ard_ecs::prelude::*;

use crate::prelude::{Window, WindowDescriptor, WindowId};

/// Container of all windows.
#[derive(Debug, Resource, Default)]
pub struct Windows {
    windows: HashMap<WindowId, Window>,
    to_create: Vec<(WindowDescriptor, WindowId)>,
}

impl Windows {
    pub fn new() -> Self {
        Windows::default()
    }

    pub fn create(&mut self, id: WindowId, descriptor: WindowDescriptor) {
        self.to_create.push((descriptor, id));
    }

    pub fn add(&mut self, window: Window) {
        self.windows.insert(window.id(), window);
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Window> {
        self.windows.values_mut()
    }

    pub fn drain_to_create(&mut self) -> std::vec::Drain<(WindowDescriptor, WindowId)> {
        self.to_create.drain(..)
    }
}
