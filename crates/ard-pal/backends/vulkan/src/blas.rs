use std::{ffi::CString, mem::ManuallyDrop, sync::Arc};

use api::{
    blas::{
        BottomLevelAccelerationStructureCreateError, BottomLevelAccelerationStructureCreateInfo,
        BottomLevelAccelerationStructureData,
    },
    buffer::Buffer,
    types::{BuildAccelerationStructureFlags, MemoryUsage, SharingMode},
    Backend,
};
use arc_swap::ArcSwap;
use ash::vk::{self, AccelerationStructureGeometryKHR, Handle, QueryType};
use crossbeam_channel::Sender;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use rustc_hash::FxHashMap;

use crate::{
    buffer::BufferRefCounter,
    job::Job,
    util::{
        garbage_collector::Garbage,
        id_gen::ResourceId,
        queries::{Queries, Query},
        usage::BufferRegion,
    },
    VulkanBackend,
};

pub struct BottomLevelAccelerationStructure {
    pub(crate) geometries: ArcSwap<Vec<AccelerationStructureGeometryKHR>>,
    pub(crate) build_ranges: ArcSwap<Vec<vk::AccelerationStructureBuildRangeInfoKHR>>,
    pub(crate) buffer_refs: ArcSwap<FxHashMap<(vk::Buffer, usize), BlasBufferRef>>,
    pub(crate) buffer: vk::Buffer,
    pub(crate) id: ResourceId,
    pub(crate) buffer_size: u64,
    pub(crate) ref_counter: BufferRefCounter,
    pub(crate) block: ManuallyDrop<Allocation>,
    pub(crate) acceleration_struct: vk::AccelerationStructureKHR,
    pub(crate) flags: BuildAccelerationStructureFlags,
    pub(crate) scratch_size: u64,
    pub(crate) sharing_mode: SharingMode,
    pub(crate) compact_size_query: Option<Query>,
    on_drop: Sender<Garbage>,
}

#[derive(Clone)]
pub(crate) struct BlasBufferRef {
    pub _ref_counter: BufferRefCounter,
    pub id: ResourceId,
    pub sharing_mode: SharingMode,
    pub aligned_size: u64,
}

impl BottomLevelAccelerationStructure {
    pub(crate) unsafe fn new(
        ctx: &VulkanBackend,
        build_info: BottomLevelAccelerationStructureCreateInfo<crate::VulkanBackend>,
    ) -> Result<Self, BottomLevelAccelerationStructureCreateError> {
        let mut allocator = ctx.allocator.lock().unwrap();

        // Create geometries. This can be used during build commands.
        let geometries;
        let buffer_refs;
        let build_ranges;

        let sizes = match build_info.data {
            BottomLevelAccelerationStructureData::Geometry(data) => {
                geometries = ArcSwap::new(Arc::new(
                    data.iter()
                        .map(|geo| {
                            let mut tris = vk::AccelerationStructureGeometryDataKHR::default();
                            tris.triangles =
                                vk::AccelerationStructureGeometryTrianglesDataKHR::builder()
                                    .vertex_format(crate::util::to_vk_format(geo.vertex_format))
                                    .vertex_data({
                                        let mut addr =
                                            geo.vertex_data.internal().device_address_const(
                                                &ctx.device,
                                                geo.vertex_data_array_element,
                                            );
                                        addr.device_address += geo.vertex_data_offset;
                                        addr
                                    })
                                    .max_vertex((geo.vertex_count - 1) as u32)
                                    .vertex_stride(geo.vertex_stride)
                                    .index_type(crate::util::to_vk_index_type(geo.index_type))
                                    .index_data({
                                        let mut addr =
                                            geo.index_data.internal().device_address_const(
                                                &ctx.device,
                                                geo.index_data_array_element,
                                            );
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
                        .collect::<Vec<_>>(),
                ));

                // Build ranges when the AS is actually built
                build_ranges = ArcSwap::new(Arc::new(
                    data.iter()
                        .map(|geo| {
                            vk::AccelerationStructureBuildRangeInfoKHR::builder()
                                .primitive_count(geo.triangle_count as u32)
                                .primitive_offset(0)
                                .first_vertex(0)
                                .transform_offset(0)
                                .build()
                        })
                        .collect::<Vec<_>>(),
                ));

                // Number of triangles per geometry.
                let num_triangles: Vec<_> =
                    data.iter().map(|geo| geo.triangle_count as u32).collect();

                // Buffer references for resource tracking.
                let mut refs = FxHashMap::default();
                data.iter().for_each(|geo| {
                    refs.entry((
                        geo.vertex_data.internal().buffer,
                        geo.vertex_data_array_element,
                    ))
                    .or_insert_with(|| BlasBufferRef {
                        _ref_counter: geo.vertex_data.internal().ref_counter.clone(),
                        id: geo.vertex_data.internal().id,
                        sharing_mode: geo.vertex_data.internal().sharing_mode,
                        aligned_size: geo.vertex_data.internal().aligned_size,
                    });

                    refs.entry((
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
                buffer_refs = ArcSwap::new(Arc::new(refs));

                // Figure out how big the BLAS needs to be
                let geos = geometries.load();
                let build_geo_info = vk::AccelerationStructureBuildGeometryInfoKHR::builder()
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                    .flags(crate::util::to_vk_as_build_flags(build_info.flags))
                    .geometries(&geos);

                ctx.as_loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &build_geo_info,
                    &num_triangles,
                )
            }
            // Size comes directly from the user
            BottomLevelAccelerationStructureData::CompactDst(size) => {
                // Empty ranges until we copy
                build_ranges = ArcSwap::new(Arc::new(Vec::default()));
                buffer_refs = ArcSwap::new(Arc::new(FxHashMap::default()));
                geometries = ArcSwap::new(Arc::new(Vec::default()));

                vk::AccelerationStructureBuildSizesInfoKHR::builder()
                    .acceleration_structure_size(size)
                    .build_scratch_size(0)
                    .update_scratch_size(0)
                    .build()
            }
        };

        // Create the buffer to hold the BLAS
        let qfi = ctx
            .queue_family_indices
            .queue_types_to_indices(build_info.queue_types);
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
        let buffer = match ctx.device.create_buffer(&buffer_create_info, None) {
            Ok(buffer) => buffer,
            Err(err) => {
                return Err(BottomLevelAccelerationStructureCreateError::Other(
                    err.to_string(),
                ))
            }
        };

        // Allocate memory
        let mem_reqs = ctx.device.get_buffer_memory_requirements(buffer);
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
                ctx.device.destroy_buffer(buffer, None);
                return Err(BottomLevelAccelerationStructureCreateError::Other(
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
            return Err(BottomLevelAccelerationStructureCreateError::Other(
                err.to_string(),
            ));
        }

        // Setup debug name is requested
        if let Some(name) = build_info.debug_name {
            if let Some((debug, _)) = &ctx.debug {
                let name = CString::new(format!("{name}_buffer")).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::BUFFER)
                    .object_handle(buffer.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .set_debug_utils_object_name(ctx.device.handle(), &name_info)
                    .unwrap();
            }
        }

        // Create the BLAS
        let create_info = vk::AccelerationStructureCreateInfoKHR::builder()
            .buffer(buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);
        let acceleration_struct = match ctx
            .as_loader
            .create_acceleration_structure(&create_info, None)
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
            id: ctx.buffer_ids.create(),
            buffer_size: sizes.acceleration_structure_size,
            block: ManuallyDrop::new(block),
            acceleration_struct,
            scratch_size: sizes.build_scratch_size,
            flags: build_info.flags,
            sharing_mode: build_info.sharing_mode,
            ref_counter: BufferRefCounter::default(),
            on_drop: ctx.garbage.sender(),
            compact_size_query: if build_info
                .flags
                .contains(BuildAccelerationStructureFlags::ALLOW_COMPACTION)
            {
                Some(
                    ctx.queries
                        .lock()
                        .unwrap()
                        .allocate_accel_struct_compact(&ctx.device),
                )
            } else {
                None
            },
        })
    }

    #[inline(always)]
    pub(crate) fn scratch_size(&self) -> u64 {
        self.scratch_size
    }

    #[inline(always)]
    pub(crate) unsafe fn compacted_size(&self, ctx: &VulkanBackend) -> u64 {
        // If we don't have a query to read, exit early
        let query = match self.compact_size_query {
            Some(query) => query,
            None => return 0,
        };

        // Wait until the last queue that the buffer was used in has finished it's work
        let resc_state = ctx.resource_state.write().unwrap();

        // NOTE: The reason we set the usage to `None` is because we have to wait for the previous
        // usage to complete. This implies that no one is using this buffer anymore and thus no
        // waits are further needed.
        if let Some(old) = resc_state.get_buffer_queue_usage(&BufferRegion {
            id: self.id,
            array_elem: 0,
        }) {
            ctx.wait_on(
                &Job {
                    ty: old.queue,
                    target_value: old.timeline_value,
                },
                None,
            );
        }

        ctx.queries
            .lock()
            .unwrap()
            .get_accel_struct_compact(&ctx.device, query)
    }

    pub(crate) unsafe fn build(
        &self,
        device: &ash::Device,
        commands: vk::CommandBuffer,
        as_loader: &ash::extensions::khr::AccelerationStructure,
        scratch: &Buffer<crate::VulkanBackend>,
        scratch_array_element: usize,
    ) {
        let ranges = self.build_ranges.load();
        let geometries = self.geometries.load();

        let build_geo_info = [vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(self.acceleration_struct)
            .flags(crate::util::to_vk_as_build_flags(self.flags))
            .geometries(&geometries)
            .scratch_data(
                scratch
                    .internal()
                    .device_address(device, scratch_array_element),
            )
            .build()];
        let infos = [ranges.as_slice()];

        as_loader.cmd_build_acceleration_structures(commands, &build_geo_info, &infos);
    }

    pub(crate) unsafe fn write_compact_size(
        &self,
        commands: vk::CommandBuffer,
        as_loader: &ash::extensions::khr::AccelerationStructure,
        queries: &Queries,
    ) {
        let query = self.compact_size_query.as_ref().unwrap();
        as_loader.cmd_write_acceleration_structures_properties(
            commands,
            &[self.acceleration_struct],
            QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR,
            queries.accel_struct_pool(query.pool),
            query.idx as u32,
        );
    }

    pub(crate) unsafe fn copy_from(
        &self,
        commands: vk::CommandBuffer,
        as_loader: &ash::extensions::khr::AccelerationStructure,
        src: &Self,
    ) {
        // Copy buffer references and build ranges
        self.buffer_refs.store(src.buffer_refs.load().clone());
        self.build_ranges.store(src.build_ranges.load().clone());

        // Copy acceleration structure
        let copy_info = vk::CopyAccelerationStructureInfoKHR::builder()
            .src(src.acceleration_struct)
            .dst(self.acceleration_struct)
            .mode(vk::CopyAccelerationStructureModeKHR::COMPACT);
        as_loader.cmd_copy_acceleration_structure(commands, &copy_info);
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
            compact_size_query: self.compact_size_query.take(),
        });
    }
}

unsafe impl Send for BottomLevelAccelerationStructure {}
unsafe impl Sync for BottomLevelAccelerationStructure {}
