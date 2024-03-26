use std::{ffi::CString, mem::ManuallyDrop};

use api::{
    acceleration_structure::{
        BottomLevelAccelerationStructureCreateError, BottomLevelAccelerationStructureCreateInfo,
    },
    buffer::Buffer,
    types::{BuildAccelerationStructureFlags, MemoryUsage, SharingMode},
};
use ash::vk::{self, AccelerationStructureGeometryKHR, Handle};
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use rustc_hash::FxHashMap;

use crate::{
    buffer::BufferRefCounter,
    util::{
        garbage_collector::Garbage,
        id_gen::{IdGenerator, ResourceId},
    },
    QueueFamilyIndices,
};

pub struct BottomLevelAccelerationStructure {
    pub(crate) geometries: Vec<AccelerationStructureGeometryKHR>,
    pub(crate) build_ranges: Vec<vk::AccelerationStructureBuildRangeInfoKHR>,
    pub(crate) buffer_refs: FxHashMap<(vk::Buffer, usize), BlasBufferRef>,
    pub(crate) buffer: vk::Buffer,
    pub(crate) id: ResourceId,
    pub(crate) buffer_size: u64,
    pub(crate) ref_counter: BufferRefCounter,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) acceleration_struct: vk::AccelerationStructureKHR,
    pub(crate) flags: BuildAccelerationStructureFlags,
    pub(crate) scratch_size: u64,
    pub(crate) sharing_mode: SharingMode,
    on_drop: Sender<Garbage>,
}

pub(crate) struct BlasBufferRef {
    pub _ref_counter: BufferRefCounter,
    pub id: ResourceId,
    pub sharing_mode: SharingMode,
    pub aligned_size: u64,
}

impl BottomLevelAccelerationStructure {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        qfi: &QueueFamilyIndices,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        on_drop: Sender<Garbage>,
        id_gen: &IdGenerator,
        allocator: &mut Allocator,
        as_loader: &ash::extensions::khr::AccelerationStructure,
        build_info: BottomLevelAccelerationStructureCreateInfo<crate::VulkanBackend>,
    ) -> Result<Self, BottomLevelAccelerationStructureCreateError> {
        // Create geometries. This can be used during build commands.
        let geometries: Vec<_> = build_info
            .geometries
            .iter()
            .map(|geo| {
                let mut tris = vk::AccelerationStructureGeometryDataKHR::default();
                tris.triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::builder()
                    .vertex_format(crate::util::to_vk_format(geo.vertex_format))
                    .vertex_data({
                        let mut addr = geo
                            .vertex_data
                            .internal()
                            .device_address_const(device, geo.vertex_data_array_element);
                        addr.device_address += geo.vertex_data_offset;
                        addr
                    })
                    .max_vertex((geo.vertex_count - 1) as u32)
                    .vertex_stride(geo.vertex_stride)
                    .index_type(crate::util::to_vk_index_type(geo.index_type))
                    .index_data({
                        let mut addr = geo
                            .index_data
                            .internal()
                            .device_address_const(device, geo.index_data_array_element);
                        addr.device_address += geo.index_data_offset;
                        addr
                    })
                    .transform_data(vk::DeviceOrHostAddressConstKHR::default())
                    .build();

                vk::AccelerationStructureGeometryKHR::builder()
                    .flags(crate::util::to_vk_geometry_flags(geo.flags))
                    .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                    .geometry(tris)
                    .build()
            })
            .collect();

        // Build ranges when the AS is actually built
        let build_ranges: Vec<_> = build_info
            .geometries
            .iter()
            .map(|geo| {
                vk::AccelerationStructureBuildRangeInfoKHR::builder()
                    .primitive_count(geo.triangle_count as u32)
                    .primitive_offset(0)
                    .first_vertex(0)
                    .transform_offset(0)
                    .build()
            })
            .collect();

        // Number of triangles per geometry.
        let num_triangles: Vec<_> = build_info
            .geometries
            .iter()
            .map(|geo| geo.triangle_count as u32)
            .collect();

        // Buffer references for resource tracking.
        let mut buffer_refs = FxHashMap::default();
        build_info.geometries.iter().for_each(|geo| {
            buffer_refs
                .entry((
                    geo.vertex_data.internal().buffer,
                    geo.vertex_data_array_element,
                ))
                .or_insert_with(|| BlasBufferRef {
                    _ref_counter: geo.vertex_data.internal().ref_counter.clone(),
                    id: geo.vertex_data.internal().id,
                    sharing_mode: geo.vertex_data.internal().sharing_mode,
                    aligned_size: geo.vertex_data.internal().aligned_size,
                });

            buffer_refs
                .entry((
                    geo.index_data.internal().buffer,
                    geo.index_data_array_element,
                ))
                .or_insert_with(|| BlasBufferRef {
                    _ref_counter: geo.index_data.internal().ref_counter.clone(),
                    id: geo.index_data.internal().id,
                    sharing_mode: geo.index_data.internal().sharing_mode,
                    aligned_size: geo.index_data.internal().aligned_size,
                });
        });

        // Figure out how big the BLAS needs to be
        let build_geo_info = vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(crate::util::to_vk_as_build_flags(build_info.flags))
            .geometries(&geometries);

        let sizes = as_loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build_geo_info,
            &num_triangles,
        );

        // Create the buffer to hold the BLAS
        let qfi = qfi.queue_types_to_indices(build_info.queue_types);
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(sizes.acceleration_structure_size)
            .usage(
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )
            .sharing_mode(if qfi.len() == 1 {
                vk::SharingMode::EXCLUSIVE
            } else {
                crate::util::to_vk_sharing_mode(build_info.sharing_mode)
            })
            .queue_family_indices(&qfi);
        let buffer = match device.create_buffer(&buffer_create_info, None) {
            Ok(buffer) => buffer,
            Err(err) => {
                return Err(BottomLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ))
            }
        };

        // Allocate memory
        let mem_reqs = device.get_buffer_memory_requirements(buffer);
        let request = AllocationCreateDesc {
            name: match &build_info.debug_name {
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
                device.destroy_buffer(buffer, None);
                return Err(BottomLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ));
            }
        };

        // Bind buffer to memory
        if let Err(err) = device.bind_buffer_memory(buffer, block.memory(), block.offset()) {
            allocator.free(block).unwrap();
            device.destroy_buffer(buffer, None);
            return Err(BottomLevelAccelerationStructureCreateError::Other(
                err.to_string(),
            ));
        }

        // Setup debug name is requested
        if let Some(name) = build_info.debug_name {
            if let Some(debug) = debug {
                let name = CString::new(format!("{name}_buffer")).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::BUFFER)
                    .object_handle(buffer.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .set_debug_utils_object_name(device.handle(), &name_info)
                    .unwrap();
            }
        }

        // Create the BLAS
        let create_info = vk::AccelerationStructureCreateInfoKHR::builder()
            .buffer(buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);
        let acceleration_struct = match as_loader.create_acceleration_structure(&create_info, None)
        {
            Ok(astruct) => astruct,
            Err(err) => {
                return Err(BottomLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ))
            }
        };

        Ok(BottomLevelAccelerationStructure {
            geometries,
            build_ranges,
            buffer_refs,
            buffer,
            id: id_gen.create(),
            buffer_size: sizes.acceleration_structure_size,
            block: ManuallyDrop::new(block),
            acceleration_struct,
            scratch_size: sizes.build_scratch_size,
            flags: build_info.flags,
            sharing_mode: build_info.sharing_mode,
            ref_counter: BufferRefCounter::default(),
            on_drop,
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
        as_loader: &ash::extensions::khr::AccelerationStructure,
        scratch: &Buffer<crate::VulkanBackend>,
        scratch_array_element: usize,
    ) {
        let build_geo_info = [vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(self.acceleration_struct)
            .flags(crate::util::to_vk_as_build_flags(self.flags))
            .geometries(&self.geometries)
            .scratch_data(
                scratch
                    .internal()
                    .device_address(device, scratch_array_element),
            )
            .build()];
        let infos = [self.build_ranges.as_slice()];

        as_loader.cmd_build_acceleration_structures(commands, &build_geo_info, &infos);
    }
}

impl Drop for BottomLevelAccelerationStructure {
    fn drop(&mut self) {
        let _ = self.on_drop.send(Garbage::AccelerationStructure {
            buffer: self.buffer,
            accelleration_struct: self.acceleration_struct,
            id: self.id,
            allocation: unsafe { ManuallyDrop::take(&mut self.block) },
            ref_counter: self.ref_counter.clone(),
        });
    }
}

unsafe impl Send for BottomLevelAccelerationStructure {}
unsafe impl Sync for BottomLevelAccelerationStructure {}
