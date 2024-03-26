use std::collections::VecDeque;

use ard_pal::prelude::Buffer;
use ard_render_base::resource::ResourceId;

const BLAS_BUILD_PER_PROCESS: usize = 16;

#[derive(Default)]
pub struct PendingBlasBuilder {
    all_pending: VecDeque<PendingBlas>,
    current: Vec<PendingBlas>,
}

pub struct PendingBlas {
    pub mesh_id: ResourceId,
    pub scratch: Box<Buffer>,
}

impl PendingBlasBuilder {
    #[inline(always)]
    pub fn current(&self) -> &[PendingBlas] {
        &self.current
    }

    #[inline(always)]
    pub fn append(&mut self, mesh_id: ResourceId, scratch: Box<Buffer>) {
        self.all_pending.push_back(PendingBlas { mesh_id, scratch });
    }

    #[inline(always)]
    pub fn build_current_list(&mut self) {
        self.current.clear();

        let rng = ..self.all_pending.len().min(BLAS_BUILD_PER_PROCESS);
        self.current = self.all_pending.drain(rng).collect();
    }
}
