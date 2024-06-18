use ard_pal::prelude::*;

use crate::{shape::DebugShapeVertex, DebugDraw};

const DEFAULT_CAP: usize = 128;

pub struct DebugVertexBuffer {
    buffer: Buffer,
    vertex_count: usize,
}

impl DebugVertexBuffer {
    pub fn new(ctx: &Context) -> Self {
        DebugVertexBuffer {
            vertex_count: 0,
            buffer: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_CAP * std::mem::size_of::<DebugShapeVertex>()) as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::VERTEX_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("debug_vertex_buffer".into()),
                },
            )
            .unwrap(),
        }
    }

    #[inline(always)]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    pub fn write_draws(&mut self, draws: &[DebugDraw]) {
        self.vertex_count = draws.iter().map(|d| d.shape.vertex_count()).sum();

        let new_size = self.vertex_count * std::mem::size_of::<DebugShapeVertex>();
        if let Some(new_buffer) = Buffer::expand(&self.buffer, new_size as u64, false) {
            self.buffer = new_buffer;
        }

        let mut view = self.buffer.write(0).unwrap();
        let slice: &mut [DebugShapeVertex] = bytemuck::cast_slice_mut(&mut view[0..new_size]);

        let mut start = 0;
        draws.iter().for_each(|draw| {
            draw.shape.write_vertices(&mut slice[start..], draw.color);
            start += draw.shape.vertex_count();
        });
    }
}
