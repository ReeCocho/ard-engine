use std::ops::DerefMut;

use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, FRAMES_IN_FLIGHT};
use ard_render_objects::set::RenderableSet;
use ard_render_si::types::*;

pub struct RenderIds {
    input: Buffer,
    output: Buffer,
}

const DEFAULT_RENDER_ID_COUNT: u64 = 1;

impl RenderIds {
    pub fn new(ctx: &Context) -> Self {
        Self {
            input: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: std::mem::size_of::<GpuObjectId>() as u64 * DEFAULT_RENDER_ID_COUNT,
                    array_elements: FRAMES_IN_FLIGHT,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("input_ids".into()),
                },
            )
            .unwrap(),
            output: Buffer::new(
                ctx.clone(),
                BufferCreateInfo {
                    size: std::mem::size_of::<u16>() as u64 * DEFAULT_RENDER_ID_COUNT,
                    array_elements: 1,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::GpuOnly,
                    queue_types: QueueTypes::MAIN,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("output_ids".into()),
                },
            )
            .unwrap(),
        }
    }

    #[inline(always)]
    pub fn input(&self) -> &Buffer {
        &self.input
    }

    #[inline(always)]
    pub fn output(&self) -> &Buffer {
        &self.output
    }

    /// Upload object IDs from a renderable set. Returns `true` if any of the internal buffers
    /// were reszied.
    pub fn upload(&mut self, frame: Frame, static_dirty: bool, set: &RenderableSet) -> bool {
        // Expand input ID buffers if needed
        let input_id_buffer_size = std::mem::size_of_val(set.ids()) as u64;
        let input_id_buffer_expanded =
            match Buffer::expand(&self.input, input_id_buffer_size, false) {
                Some(mut new_buffer) => {
                    std::mem::swap(&mut self.input, &mut new_buffer);
                    true
                }
                None => false,
            };

        // Write in object IDs
        let mut id_view = self.input.write(usize::from(frame)).unwrap();
        let id_slice = bytemuck::cast_slice_mut::<_, GpuObjectId>(id_view.deref_mut());

        // Expand output ID buffer if needed
        let output_id_buffer_size = (set.ids().len() as u64 + set.max_meshlet_count() as u64)
            * std::mem::size_of::<u16>() as u64;
        let output_id_buffer_expanded =
            match Buffer::expand(&self.output, output_id_buffer_size, false) {
                Some(mut new_buffer) => {
                    std::mem::swap(&mut self.output, &mut new_buffer);
                    true
                }
                None => false,
            };

        // Write in static ids if they were modified
        if input_id_buffer_expanded || output_id_buffer_expanded || static_dirty {
            id_slice[set.static_object_ranges().opaque.clone()]
                .copy_from_slice(&set.ids()[set.static_object_ranges().opaque.clone()]);
            id_slice[set.static_object_ranges().alpha_cutout.clone()]
                .copy_from_slice(&set.ids()[set.static_object_ranges().alpha_cutout.clone()]);
        }

        // Write in dynamic object IDs
        id_slice[set.dynamic_object_ranges().opaque.clone()]
            .copy_from_slice(&set.ids()[set.dynamic_object_ranges().opaque.clone()]);
        id_slice[set.dynamic_object_ranges().alpha_cutout.clone()]
            .copy_from_slice(&set.ids()[set.dynamic_object_ranges().alpha_cutout.clone()]);

        // Write in transparent object IDs
        id_slice[set.transparent_object_range().clone()]
            .copy_from_slice(&set.ids()[set.transparent_object_range().clone()]);

        input_id_buffer_expanded || output_id_buffer_expanded
    }
}
