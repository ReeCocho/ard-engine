use std::ffi::CString;

use api::compute_pipeline::{ComputePipelineCreateError, ComputePipelineCreateInfo};
use ash::vk::{self, Handle};
use crossbeam_channel::Sender;

use crate::util::garbage_collector::Garbage;

pub struct ComputePipeline {
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) pipeline: vk::Pipeline,
    garbage: Sender<Garbage>,
}

impl ComputePipeline {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        garbage: Sender<Garbage>,
        create_info: ComputePipelineCreateInfo<crate::VulkanBackend>,
    ) -> Result<Self, ComputePipelineCreateError> {
        let push_constant_ranges = create_info.push_constants_size.map(|size| {
            [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                offset: 0,
                size,
            }]
        });

        // Create the layout
        let mut layouts = Vec::with_capacity(create_info.layouts.len());
        for layout in &create_info.layouts {
            layouts.push(layout.internal().layout);
        }
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .push_constant_ranges(match &push_constant_ranges {
                Some(range) => range,
                None => &[],
            })
            .build();
        let layout = device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        // Create the pipeline
        let map_entries = [
            vk::SpecializationMapEntry::builder()
                .constant_id(0)
                .offset(0)
                .size(std::mem::size_of::<u32>())
                .build(),
            vk::SpecializationMapEntry::builder()
                .constant_id(1)
                .offset(std::mem::size_of::<u32>() as u32)
                .size(std::mem::size_of::<u32>())
                .build(),
            vk::SpecializationMapEntry::builder()
                .constant_id(2)
                .offset(2 * std::mem::size_of::<u32>() as u32)
                .size(std::mem::size_of::<u32>())
                .build(),
        ];
        let values = [
            create_info.work_group_size.0,
            create_info.work_group_size.1,
            create_info.work_group_size.2,
        ];

        let specialization = vk::SpecializationInfo::builder()
            .map_entries(&map_entries)
            .data(bytemuck::cast_slice(&values))
            .build();

        let entry_point = std::ffi::CString::new("main").unwrap();
        let stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(create_info.module.internal().module)
            .name(&entry_point)
            .specialization_info(&specialization)
            .build();

        let pipeline_info = [vk::ComputePipelineCreateInfo::builder()
            .stage(stage)
            .layout(layout)
            .build()];

        let pipeline = match device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &pipeline_info,
            None,
        ) {
            Ok(pipeline) => pipeline[0],
            Err((_, err)) => return Err(ComputePipelineCreateError::Other(err.to_string())),
        };

        // Name the pipeline if needed
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = debug {
                let name = CString::new(name).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::PIPELINE)
                    .object_handle(pipeline.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .debug_utils_set_object_name(device.handle(), &name_info)
                    .unwrap();
            }
        }

        Ok(Self {
            pipeline,
            layout,
            garbage,
        })
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        let _ = self.garbage.send(Garbage::Pipeline(self.pipeline));
        let _ = self.garbage.send(Garbage::PipelineLayout(self.layout));
    }
}
