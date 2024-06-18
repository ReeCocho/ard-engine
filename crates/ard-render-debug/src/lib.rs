pub mod buffer;
pub mod shape;

use ard_ecs::prelude::*;
use ard_math::Vec4;
use shape::Shape;

#[derive(Resource, Default)]
pub struct DebugDrawing {
    draws: Vec<DebugDraw>,
}

#[derive(Debug, Clone, Copy)]
pub struct DebugDraw {
    pub color: Vec4,
    pub shape: Shape,
}

impl DebugDrawing {
    #[inline(always)]
    pub fn draws(&self) -> &[DebugDraw] {
        &self.draws
    }

    #[inline(always)]
    pub fn draw(&mut self, draw: DebugDraw) {
        self.draws.push(draw);
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.draws.clear()
    }
}
