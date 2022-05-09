use ash::vk;

#[derive(Clone)]
pub(crate) struct Layouts {
    pub depth_only_pipeline_layout: vk::PipelineLayout,
    pub opaque_pipeline_layout: vk::PipelineLayout,
}

impl Layouts {
    pub unsafe fn new(
        device: &ash::Device,
        global_layout: vk::DescriptorSetLayout,
        textures_layout: vk::DescriptorSetLayout,
        materials_layout: vk::DescriptorSetLayout,
        camera_layout: vk::DescriptorSetLayout,
    ) -> Self {
        let layouts = [
            global_layout,
            textures_layout,
            camera_layout,
            materials_layout,
        ];

        let create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .build();

        let opaque_pipeline_layout = device
            .create_pipeline_layout(&create_info, None)
            .expect("unable to create opaque pipeline layout");

        let depth_only_pipeline_layout = device
            .create_pipeline_layout(&create_info, None)
            .expect("unable to create depth only pipeline layout");

        Self {
            opaque_pipeline_layout,
            depth_only_pipeline_layout,
        }
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        device.destroy_pipeline_layout(self.depth_only_pipeline_layout, None);
        device.destroy_pipeline_layout(self.opaque_pipeline_layout, None);
    }
}
