use ard_ecs::prelude::*;

/// Frame index.
#[derive(Debug, Copy, Clone)]
pub struct Frame(usize);

impl From<usize> for Frame {
    fn from(value: usize) -> Self {
        Frame(value)
    }
}

impl From<Frame> for usize {
    fn from(value: Frame) -> Self {
        value.0
    }
}

/// Event sent into the render ECS to signal that preprocessing can being
#[derive(Copy, Clone, Event)]
pub struct RenderPreprocessing(pub Frame);
