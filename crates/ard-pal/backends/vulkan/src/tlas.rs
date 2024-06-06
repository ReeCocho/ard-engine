use std::{ffi::CString, mem::ManuallyDrop};

use api::{
    buffer::Buffer,
    tlas::{TopLevelAccelerationStructureCreateError, TopLevelAccelerationStructureCreateInfo},
    types::{BuildAccelerationStructureFlags, MemoryUsage, SharingMode},
};
use ash::vk;
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};

use crate::{
    buffer::BufferRefCounter,
    util::{garbage_collector::Garbage, id_gen::ResourceId},
    VulkanBackend,
};

pub struct TopLevelAccelerationStructure {
    pub(crate) buffer: vk::Buffer,
    pub(crate) id: ResourceId,
    pub(crate) buffer_size: u64,
    pub(crate) ref_counter: BufferRefCounter,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) acceleration_struct: vk::AccelerationStructureKHR,
    pub(crate) sharing_mode: SharingMode,
    pub(crate) scratch_size: u64,
    pub(crate) flags: BuildAccelerationStructureFlags,
    on_drop: Sender<Garbage>,
}

impl TopLevelAccelerationStructure {
    pub(crate) unsafe fn new(
        ctx: &VulkanBackend,
        create_info: TopLevelAccelerationStructureCreateInfo,
    ) -> Result<Self, TopLevelAccelerationStructureCreateError> {
        let mut allocator = ctx.allocator.lock().unwrap();

        let instances =
            vk::AccelerationStructureGeometryInstancesDataKHR::default().array_of_pointers(true);

        let geo = [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            // TODO: Make configurable
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { instances })];

        let geo_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .flags(crate::util::to_vk_as_build_flags(create_info.flags))
            .geometries(&geo);
        let prim_count = [create_info.capacity as u32];
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        ctx.as_loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &geo_info,
            &prim_count,
            &mut sizes,
        );

        // Create the buffer to hold the TLAS
        let qfi = ctx
            .queue_family_indices
            .queue_types_to_indices(create_info.queue_types);
        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(sizes.acceleration_structure_size)
            .usage(
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )
            .sharing_mode(if qfi.len() == 1 {
                vk::SharingMode::EXCLUSIVE
            } else {
                crate::util::to_vk_sharing_mode(create_info.sharing_mode)
            })
            .queue_family_indices(&qfi);
        let buffer = match ctx.device.create_buffer(&buffer_create_info, None) {
            Ok(buffer) => buffer,
            Err(err) => {
                return Err(TopLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ))
            }
        };

        // Allocate memory
        let mem_reqs = ctx.device.get_buffer_memory_requirements(buffer);
        let request = AllocationCreateDesc {
            name: match &create_info.debug_name {
                Some(name) => name,
                None => "unnamed_buffer",
            },
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            requirements: mem_reqs,
            location: crate::util::to_gpu_allocator_memory_location(MemoryUsage::GpuOnly),
            linear: true,
        };
        let block = match allocator.allocate(&request) {
            Ok(block) => block,
            Err(err) => {
                ctx.device.destroy_buffer(buffer, None);
                return Err(TopLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ));
            }
        };

        // Bind buffer to memory
        if let Err(err) = ctx
            .device
            .bind_buffer_memory(buffer, block.memory(), block.offset())
        {
            allocator.free(block).unwrap();
            ctx.device.destroy_buffer(buffer, None);
            return Err(TopLevelAccelerationStructureCreateError::Other(
                err.to_string(),
            ));
        }

        // Setup debug name is requested
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = &ctx.debug {
                let name = CString::new(format!("{name}_buffer")).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(buffer)
                    .object_name(&name);

                debug
                    .device
                    .set_debug_utils_object_name(&name_info)
                    .unwrap();
            }
        }

        // Create the BLAS
        let tlas_create_info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);
        let acceleration_struct = match ctx
            .as_loader
            .create_acceleration_structure(&tlas_create_info, None)
        {
            Ok(astruct) => astruct,
            Err(err) => {
                return Err(TopLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ))
            }
        };

        Ok(TopLevelAccelerationStructure {
            on_drop: ctx.garbage.sender(),
            buffer,
            id: ctx.buffer_ids.create(),
            buffer_size: sizes.acceleration_structure_size,
            ref_counter: BufferRefCounter::default(),
            block: ManuallyDrop::new(block),
            acceleration_struct,
            sharing_mode: create_info.sharing_mode,
            scratch_size: sizes.build_scratch_size,
            flags: create_info.flags,
        })
    }

    #[inline(always)]
    pub(crate) fn scratch_size(&self) -> u64 {
        self.scratch_size
    }

    pub(crate) unsafe fn build(
        &self,
        device: &ash::Device,
        commands: vk::CommandBuffer,
        as_loader: &ash::khr::acceleration_structure::Device,
        instance_count: usize,
        scratch: &Buffer<crate::VulkanBackend>,
        scratch_array_element: usize,
        src: &Buffer<crate::VulkanBackend>,
        src_array_element: usize,
    ) {
        let instances = vk::AccelerationStructureGeometryInstancesDataKHR::default()
            .array_of_pointers(true)
            .data(
                src.internal()
                    .device_address_const(device, src_array_element),
            );

        let geo = [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            // TODO: Make configurable
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { instances })];

        let build_geo_info = [vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(self.acceleration_struct)
            .flags(crate::util::to_vk_as_build_flags(self.flags))
            .geometries(&geo)
            .scratch_data(
                scratch
                    .internal()
                    .device_address(device, scratch_array_element),
            )];

        let build_range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(instance_count as u32)
            .primitive_offset(0)
            .first_vertex(0)
            .transform_offset(0)];

        let infos = [build_range.as_slice()];

        as_loader.cmd_build_acceleration_structures(commands, &build_geo_info, &infos);
    }
}

impl Drop for TopLevelAccelerationStructure {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::AccelerationStructure {
            buffer: self.buffer,
            accelleration_struct: self.acceleration_struct,
            id: self.id,
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
            compact_size_query: None,
        });
    }
}

unsafe impl Send for TopLevelAccelerationStructure {}
unsafe impl Sync for TopLevelAccelerationStructure {}
