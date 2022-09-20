use std::{collections::HashMap, ffi::CString};

use super::fast_int_hasher::FIHashMap;
use api::descriptor_set::DescriptorSetLayoutCreateInfo;
use ash::vk::{self, Handle};
use fxhash::FxHashMap;

/// Default number of sets per pool.
const SETS_PER_POOL: usize = 16;

#[derive(Default)]
pub(crate) struct DescriptorPools {
    pools: FxHashMap<DescriptorSetLayoutCreateInfo, DescriptorPool>,
    layout_to_create_info: FIHashMap<vk::DescriptorSetLayout, DescriptorSetLayoutCreateInfo>,
}

pub(crate) struct DescriptorPool {
    /// # Note
    /// The layout is held in an array for convenience when allocating pools.
    layout: [vk::DescriptorSetLayout; 1],
    /// Pools to allocate sets from.
    pools: Vec<vk::DescriptorPool>,
    /// Current number of sets allocated from the top pool.
    size: usize,
    /// Free list of descriptor sets.
    free: Vec<vk::DescriptorSet>,
    /// Pool sizes to use when making a new descriptor pool.
    sizes: Vec<vk::DescriptorPoolSize>,
}

impl DescriptorPools {
    #[inline]
    pub unsafe fn get(
        &mut self,
        device: &ash::Device,
        create_info: DescriptorSetLayoutCreateInfo,
    ) -> &mut DescriptorPool {
        if !self.pools.contains_key(&create_info) {
            let pool = DescriptorPool::new(device, &create_info);
            self.layout_to_create_info
                .insert(pool.layout[0], create_info.clone());
            self.pools.insert(create_info.clone(), pool);
        }
        self.pools.get_mut(&create_info).unwrap()
    }

    #[inline]
    pub unsafe fn get_by_layout(
        &mut self,
        layout: vk::DescriptorSetLayout,
    ) -> Option<&mut DescriptorPool> {
        let ci = match self.layout_to_create_info.get(&layout) {
            Some(ci) => ci,
            None => return None,
        };
        self.pools.get_mut(ci)
    }

    pub unsafe fn release(&mut self, device: &ash::Device) {
        for (_, mut pool) in self.pools.drain() {
            pool.release(device);
        }
    }
}

impl DescriptorPool {
    pub unsafe fn new(device: &ash::Device, create_info: &DescriptorSetLayoutCreateInfo) -> Self {
        // Convert the api layout into a vulkan layout
        let mut bindings = Vec::default();
        for binding in &create_info.bindings {
            bindings.push(
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(binding.binding)
                    .descriptor_count(binding.count as u32)
                    .descriptor_type(super::to_vk_descriptor_type(binding.ty))
                    .stage_flags(crate::util::to_vk_shader_stage(binding.stage))
                    .build(),
            );
        }

        let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        // Create the layout
        let layout = device
            .create_descriptor_set_layout(&create_info, None)
            .unwrap();

        // Create the required pool sizes
        // Maps descriptor types to the number required
        let mut pool_sizes = HashMap::<vk::DescriptorType, u32>::default();
        for i in 0..create_info.binding_count {
            let binding = create_info.p_bindings.add(i as usize).as_ref().unwrap();

            // Update pool size
            *pool_sizes.entry(binding.descriptor_type).or_default() += binding.descriptor_count;
        }

        // Convert map of pool sizes into a vec
        let sizes = pool_sizes
            .into_iter()
            .map(|(ty, count)| vk::DescriptorPoolSize {
                ty,
                descriptor_count: count * SETS_PER_POOL as u32,
            })
            .collect::<Vec<_>>();

        Self {
            layout: [layout],
            pools: Vec::default(),
            size: 0,
            free: Vec::default(),
            sizes,
        }
    }

    #[inline(always)]
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout[0]
    }

    #[inline]
    pub fn free(&mut self, set: vk::DescriptorSet) {
        self.free.push(set);
    }

    pub unsafe fn allocate(
        &mut self,
        device: &ash::Device,
        debug: Option<&ash::extensions::ext::DebugUtils>,
        name: Option<String>,
    ) -> vk::DescriptorSet {
        let set = match self.free.pop() {
            Some(free) => free,
            None => {
                // Allocate a new pool if required
                if self.size == 0 {
                    self.size = SETS_PER_POOL;
                    self.pools.push({
                        let create_info = vk::DescriptorPoolCreateInfo::builder()
                            .max_sets(self.size as u32)
                            .pool_sizes(&self.sizes)
                            .build();
                        device.create_descriptor_pool(&create_info, None).unwrap()
                    });
                }

                // Allocate new set
                self.size -= 1;
                let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                    .descriptor_pool(*self.pools.last().unwrap())
                    .set_layouts(&self.layout)
                    .build();
                device.allocate_descriptor_sets(&alloc_info).unwrap()[0]
            }
        };

        // Name the set if needed
        if let Some(name) = name {
            if let Some(debug) = debug {
                let name = CString::new(name).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                    .object_type(vk::ObjectType::DESCRIPTOR_SET)
                    .object_handle(set.as_raw())
                    .object_name(&name)
                    .build();

                debug
                    .debug_utils_set_object_name(device.handle(), &name_info)
                    .unwrap();
            }
        }

        set
    }

    pub unsafe fn release(&mut self, device: &ash::Device) {
        for pool in self.pools.drain(..) {
            device.destroy_descriptor_pool(pool, None);
        }
        device.destroy_descriptor_set_layout(self.layout[0], None);
    }
}
