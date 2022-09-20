use super::fast_int_hasher::FIHashMap;
use ash::vk;

#[derive(Default)]
pub(crate) struct PipelineCache {
    /// Given a pipeline layout and render pass, produces a unique matching pipeline.
    pipelines: FIHashMap<vk::PipelineLayout, FIHashMap<vk::RenderPass, vk::Pipeline>>,
}

impl PipelineCache {
    #[inline(always)]
    pub fn count(&self, layout: vk::PipelineLayout) -> usize {
        match self.pipelines.get(&layout) {
            Some(passes) => passes.len(),
            None => 0,
        }
    }

    #[inline(always)]
    pub fn get(&self, layout: vk::PipelineLayout, pass: vk::RenderPass) -> Option<vk::Pipeline> {
        match self.pipelines.get(&layout) {
            Some(passes) => passes.get(&pass).map(|p| *p),
            None => None,
        }
    }

    #[inline(always)]
    pub fn insert(
        &mut self,
        layout: vk::PipelineLayout,
        pass: vk::RenderPass,
        pipeline: vk::Pipeline,
    ) {
        *self
            .pipelines
            .entry(layout)
            .or_default()
            .entry(pass)
            .or_default() = pipeline;
    }

    pub unsafe fn release(&mut self, device: &ash::Device, layout: vk::PipelineLayout) {
        if let Some(mut passes) = self.pipelines.remove(&layout) {
            for (_, pipeline) in passes.drain() {
                device.destroy_pipeline(pipeline, None);
            }
        }
    }

    pub unsafe fn release_all(&mut self, device: &ash::Device) {
        for (_, mut passes) in self.pipelines.drain() {
            for (_, pipeline) in passes.drain() {
                device.destroy_pipeline(pipeline, None);
            }
        }
    }
}
