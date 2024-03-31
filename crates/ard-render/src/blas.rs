use std::{collections::VecDeque, vec::Drain};

use ard_pal::prelude::{
    BottomLevelAccelerationStructure, BottomLevelAccelerationStructureCreateInfo,
    BottomLevelAccelerationStructureData, Buffer, BuildAccelerationStructureFlags, Context,
    QueueTypes, SharingMode,
};
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
    FRAMES_IN_FLIGHT,
};
use ard_render_meshes::mesh::MeshResource;

const BLAS_BUILD_PER_PROCESS: usize = 16;

#[derive(Default)]
pub struct PendingBlasBuilder {
    /// Meshes pending BLAS construction.
    new_pending: VecDeque<PendingBlasBuild>,
    /// Meshes pending BLAS compaction.
    compact_pending: [Vec<PendingBlasCompact>; FRAMES_IN_FLIGHT],
    /// Meshes that have been compacted and need to have their BLAS' swapped out.
    swap_pending: [Vec<PendingBlasSwap>; FRAMES_IN_FLIGHT],
    /// The meshes to build BLAS' for this frame.
    to_build: Vec<PendingBlasBuild>,
}

pub struct PendingBlasBuild {
    pub mesh_id: ResourceId,
    pub scratch: Box<Buffer>,
}

pub struct PendingBlasCompact {
    pub mesh_id: ResourceId,
    pub dst: Option<BottomLevelAccelerationStructure>,
}

pub struct PendingBlasSwap {
    pub mesh_id: ResourceId,
    pub new_blas: BottomLevelAccelerationStructure,
}

impl PendingBlasBuilder {
    /// List of BLAS' to build this frame.
    #[inline(always)]
    pub fn to_build(&self) -> &[PendingBlasBuild] {
        &self.to_build
    }

    /// List of BLAS' to compact this frame.
    pub fn to_compact(&self, frame: Frame) -> &[PendingBlasCompact] {
        &self.compact_pending[usize::from(frame)]
    }

    /// List of BLAS' to swap out.
    pub fn to_swap(&mut self, frame: Frame) -> Drain<PendingBlasSwap> {
        self.swap_pending[usize::from(frame)].drain(..)
    }

    /// Appends a new mesh and it's scratch buffer to have its BLAS built and then compacted.
    #[inline(always)]
    pub fn append(&mut self, mesh_id: ResourceId, scratch: Box<Buffer>) {
        self.new_pending
            .push_back(PendingBlasBuild { mesh_id, scratch });
    }

    /// Takes some pending meshes and creates a list of them to build.
    #[inline(always)]
    pub fn build_current_lists(
        &mut self,
        frame: Frame,
        ctx: &Context,
        meshes: &ResourceAllocator<MeshResource>,
    ) {
        // Take some new meshes and put them into the "to build" list
        self.to_build.clear();
        let rng = ..self.new_pending.len().min(BLAS_BUILD_PER_PROCESS);
        self.to_build = self.new_pending.drain(rng).collect();

        // Take meshes that were built last frame and construct their BLAS destinations.
        self.compact_pending[usize::from(frame)]
            .iter_mut()
            .for_each(|pending| {
                let compact_size = match meshes.get(pending.mesh_id) {
                    Some(mesh) => mesh.blas.compacted_size(),
                    None => return,
                };

                pending.dst = Some(
                    BottomLevelAccelerationStructure::new(
                        ctx.clone(),
                        BottomLevelAccelerationStructureCreateInfo {
                            flags: BuildAccelerationStructureFlags::PREFER_FAST_TRACE,
                            data: BottomLevelAccelerationStructureData::CompactDst(compact_size),
                            queue_types: QueueTypes::MAIN,
                            sharing_mode: SharingMode::Exclusive,
                            debug_name: Some("mesh_blas_compact".into()),
                        },
                    )
                    .unwrap(),
                );
            });
    }

    /// Take this frames BLAS' that were built/compacted and construct new lists.
    pub fn build_next_frame_lists(&mut self, frame: Frame) {
        // Take meshes that were compacted and put them into the "to swap" list
        let compact_pending = &mut self.compact_pending[usize::from(frame)];
        let swap_pending = &mut self.swap_pending[usize::from(frame)];
        for mut compacted in compact_pending.drain(..) {
            let new_blas = match compacted.dst.take() {
                Some(new_blas) => new_blas,
                None => continue,
            };

            swap_pending.push(PendingBlasSwap {
                mesh_id: compacted.mesh_id,
                new_blas,
            });
        }

        // Take the meshes that were just built and set them up for compaction next frame
        for pending_build in self.to_build.drain(..) {
            compact_pending.push(PendingBlasCompact {
                mesh_id: pending_build.mesh_id,
                dst: None,
            });
        }
    }
}
