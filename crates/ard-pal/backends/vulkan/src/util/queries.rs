use ash::vk;

const QUERIES_PER_POOL: u32 = 128;

#[derive(Default)]
pub struct Queries {
    accel_struct_compact_pools: Vec<vk::QueryPool>,
    accel_struct_compact_free: Vec<Query>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Query {
    pub pool: usize,
    pub idx: usize,
}

impl Queries {
    #[inline(always)]
    pub fn accel_struct_pool(&self, idx: usize) -> vk::QueryPool {
        self.accel_struct_compact_pools[idx]
    }

    pub unsafe fn get_accel_struct_compact(&self, device: &ash::Device, query: Query) -> u64 {
        let mut res = [vk::DeviceSize::default()];
        device
            .get_query_pool_results(
                self.accel_struct_compact_pools[query.pool],
                query.idx as u32,
                1,
                &mut res,
                vk::QueryResultFlags::empty(),
            )
            .unwrap();
        res[0]
    }

    pub unsafe fn allocate_accel_struct_compact(&mut self, device: &ash::Device) -> Query {
        // Try to get a free query
        if let Some(free) = self.accel_struct_compact_free.pop() {
            return free;
        }

        // Otherwise, we need to make a new pool
        let create_info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR)
            .query_count(QUERIES_PER_POOL);
        let pool = device.create_query_pool(&create_info, None).unwrap();
        device.reset_query_pool(pool, 0, QUERIES_PER_POOL);

        // Reserve free slots (except the first, which we're returning)
        let ret = Query {
            pool: self.accel_struct_compact_pools.len(),
            idx: 0,
        };

        (1..QUERIES_PER_POOL as usize).for_each(|i| {
            self.accel_struct_compact_free.push(Query {
                pool: self.accel_struct_compact_pools.len(),
                idx: i,
            })
        });

        self.accel_struct_compact_pools.push(pool);

        ret
    }

    pub unsafe fn free_accel_struct_compact(&mut self, device: &ash::Device, query: Query) {
        device.reset_query_pool(
            self.accel_struct_compact_pools[query.pool],
            query.idx as u32,
            1,
        );
        self.accel_struct_compact_free.push(query);
    }

    pub unsafe fn release(&self, device: &ash::Device) {
        self.accel_struct_compact_pools.iter().for_each(|pool| {
            device.destroy_query_pool(*pool, None);
        });
    }
}
