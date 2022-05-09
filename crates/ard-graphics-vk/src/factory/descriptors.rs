use std::collections::HashMap;

use ash::vk;

use crate::prelude::*;

/// Utility to manage allocating and freeing descriptors of a particular layout.
pub(crate) struct DescriptorPool {
    ctx: GraphicsContext,
    /// Layout to create descriptor sets with.
    ///
    /// # Note
    /// This is an array of one element for convenience when allocating pools.
    layout: [vk::DescriptorSetLayout; 1],
    /// List of pools to allocate sets from.
    pools: Vec<vk::DescriptorPool>,
    /// Current number of allocated sets.
    size: usize,
    /// List of free descriptor sets.
    free: Vec<vk::DescriptorSet>,
    /// Number of descriptor sets per pool.
    max_per_pool: usize,
    /// List of pool sizes used when making new descriptor pools.
    pool_sizes: Vec<vk::DescriptorPoolSize>,
}

impl DescriptorPool {
    /// Creates a descriptor pool manager given creation info for a new layout.
    pub unsafe fn new(
        ctx: &GraphicsContext,
        create_info: &vk::DescriptorSetLayoutCreateInfo,
        max_per_pool: usize,
    ) -> Self {
        // Create the descriptor set layout
        let layout = ctx
            .0
            .device
            .create_descriptor_set_layout(create_info, None)
            .expect("Unable to create descriptor set layout");

        // Create the required pool sizes

        // Maps descriptor types to the number required
        let mut pool_sizes = HashMap::<vk::DescriptorType, u32>::default();

        for i in 0..create_info.binding_count {
            let binding = create_info
                .p_bindings
                .add(i as usize)
                .as_ref()
                .expect("Unable to dereference binding");

            // Update pool size
            *pool_sizes.entry(binding.descriptor_type).or_default() += binding.descriptor_count;
        }

        // Convert map of pool sizes into a vec
        let pool_sizes = {
            let mut new_sizes = Vec::with_capacity(pool_sizes.len());
            for (ty, count) in pool_sizes {
                new_sizes.push(vk::DescriptorPoolSize {
                    ty,
                    descriptor_count: count * max_per_pool as u32,
                });
            }
            new_sizes
        };

        Self {
            ctx: ctx.clone(),
            layout: [layout],
            pools: Vec::default(),
            size: 0,
            free: Vec::default(),
            max_per_pool,
            pool_sizes,
        }
    }

    #[inline]
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout[0]
    }

    /// Allocate a new descriptor set.
    pub unsafe fn allocate(&mut self) -> vk::DescriptorSet {
        // See if we have a free set.
        if let Some(free) = self.free.pop() {
            free
        }
        // No free sets. We must make one.
        else {
            // See if we need to make a new pool
            if self.size % self.max_per_pool == 0 {
                self.pools.push({
                    let create_info = vk::DescriptorPoolCreateInfo::builder()
                        .max_sets(self.max_per_pool as u32)
                        .pool_sizes(&self.pool_sizes)
                        .build();

                    self.ctx
                        .0
                        .device
                        .create_descriptor_pool(&create_info, None)
                        .expect("Unable to create descriptor pool")
                })
            }

            // Allocate a new set
            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(self.pools[self.pools.len() - 1])
                .set_layouts(&self.layout)
                .build();

            self.ctx
                .0
                .device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Unable to allocate descriptor set")[0]
        }
    }

    /// Free a descriptor set.
    pub fn free(&mut self, set: vk::DescriptorSet) {
        self.free.push(set);
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        // Destroy layout
        unsafe {
            self.ctx
                .0
                .device
                .destroy_descriptor_set_layout(self.layout[0], None);
        }

        // Destroy pools
        for pool in self.pools.drain(..) {
            unsafe {
                self.ctx.0.device.destroy_descriptor_pool(pool, None);
            }
        }
    }
}
