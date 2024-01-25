use ard_pal::prelude::*;
use ard_render_base::ecs::Frame;
use ard_render_si::types::*;

use crate::bins::DrawBin;

pub const DEFAULT_DRAW_CALL_CAP: usize = 1;

/// A source of draw groups that can be used to generate draw calls.
pub struct OutputDrawCalls {
    calls_buffer: Buffer,
    counts_buffer: Buffer,
    call_count: usize,
}

impl OutputDrawCalls {
    pub fn new(ctx: &Context, frames_in_flight: usize) -> Self {
        Self {
            calls_buffer: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<GpuDrawCall>()) as u64,
                    array_elements: frames_in_flight * 2,
                    buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("dst_draw_calls".into()),
                },
            )
            .unwrap(),
            counts_buffer: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: (DEFAULT_DRAW_CALL_CAP * std::mem::size_of::<GpuDrawBinCount>()) as u64,
                    array_elements: frames_in_flight * 2,
                    buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("dst_draw_counts".into()),
                },
            )
            .unwrap(),
            call_count: DEFAULT_DRAW_CALL_CAP,
        }
    }

    #[inline(always)]
    pub fn draw_counts_buffer(&self, frame: Frame, use_alternate: bool) -> (&Buffer, usize) {
        (
            &self.counts_buffer,
            (usize::from(frame) * 2) + use_alternate as usize,
        )
    }

    #[inline(always)]
    pub fn last_draw_count_buffer(&self, frame: Frame, use_alternate: bool) -> (&Buffer, usize) {
        (
            &self.counts_buffer,
            (usize::from(frame) * 2) + !use_alternate as usize,
        )
    }

    #[inline(always)]
    pub fn draw_call_buffer(&self, frame: Frame, use_alternate: bool) -> (&Buffer, usize) {
        (
            &self.calls_buffer,
            (usize::from(frame) * 2) + use_alternate as usize,
        )
    }

    #[inline(always)]
    pub fn last_draw_call_buffer(&self, frame: Frame, use_alternate: bool) -> (&Buffer, usize) {
        (
            &self.calls_buffer,
            (usize::from(frame) * 2) + !use_alternate as usize,
        )
    }

    pub fn preallocate(&mut self, call_count: usize) {
        if call_count <= self.call_count {
            return;
        }

        self.call_count = call_count;

        let new_call_cap = (call_count * std::mem::size_of::<GpuDrawCall>()) as u64;
        if let Some(new_buff) = Buffer::expand(&self.calls_buffer, new_call_cap, false) {
            self.calls_buffer = new_buff;
        }
    }

    pub fn upload_counts(&mut self, bins: &[DrawBin], frame: Frame, use_alternate: bool) {
        let idx = (usize::from(frame) * 2) + use_alternate as usize;

        let count_buffer_size = (bins.len() * std::mem::size_of::<GpuDrawBinCount>()) as u64;
        if let Some(buffer) = Buffer::expand(&self.counts_buffer, count_buffer_size, false) {
            self.counts_buffer = buffer;
        }

        let mut count_view = self.counts_buffer.write(idx).unwrap();
        for (i, bin) in bins.iter().enumerate() {
            count_view.set_as_array(
                GpuDrawBinCount {
                    count: 0,
                    start: bin.offset as u32,
                },
                i,
            );
        }
    }
}
