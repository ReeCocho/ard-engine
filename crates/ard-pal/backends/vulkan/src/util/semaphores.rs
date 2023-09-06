use ash::vk;

use super::fast_int_hasher::FIHashMap;

#[derive(Default)]
pub(crate) struct SemaphoreTracker {
    wait_semaphores: FIHashMap<vk::Semaphore, WaitInfo>,
    signal_semaphores: FIHashMap<vk::Semaphore, Option<u64>>,
}

pub(crate) struct OutSemaphores {
    pub waits: Vec<(vk::Semaphore, WaitInfo)>,
    pub signals: Vec<(vk::Semaphore, Option<u64>)>,
}

#[derive(Copy, Clone)]
pub(crate) struct WaitInfo {
    pub value: Option<u64>,
    pub stage: vk::PipelineStageFlags,
}

impl SemaphoreTracker {
    #[inline(always)]
    pub fn register_wait(&mut self, semaphore: vk::Semaphore, new_info: WaitInfo) {
        let info = self.wait_semaphores.entry(semaphore).or_insert(WaitInfo {
            value: None,
            stage: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        });
        if info.value.unwrap_or_default() < new_info.value.unwrap_or_default() {
            info.value = new_info.value;
        }
        if crate::util::rank_pipeline_stage(new_info.stage)
            < crate::util::rank_pipeline_stage(info.stage)
        {
            info.stage = new_info.stage;
        }
    }

    #[inline(always)]
    pub fn register_signal(&mut self, semaphore: vk::Semaphore, value: Option<u64>) {
        let old_value = self.signal_semaphores.entry(semaphore).or_insert(value);
        if old_value.unwrap_or_default() < value.unwrap_or_default() {
            *old_value = value;
        }
    }

    #[inline(always)]
    pub fn finish(self) -> OutSemaphores {
        OutSemaphores {
            waits: self
                .wait_semaphores
                .into_iter()
                .map(|(k, v)| (k, v))
                .collect(),
            signals: self
                .signal_semaphores
                .into_iter()
                .map(|(k, v)| (k, v))
                .collect(),
        }
    }
}
