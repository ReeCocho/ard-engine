use ard_ecs::entity::Entity;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct MappedEntity(pub u32);

#[derive(Default)]
pub struct EntityMap {
    src_to_dst: FxHashMap<Entity, MappedEntity>,
    dst_to_src: Vec<Entity>,
}

impl EntityMap {
    pub fn new_from_entities(entities: &[Entity]) -> Self {
        let mut s = Self::default();
        entities.iter().for_each(|e| {
            s.src_to_dst.entry(*e).or_insert_with(|| {
                let new_id = MappedEntity(s.dst_to_src.len() as u32);
                s.dst_to_src.push(*e);
                new_id
            });
        });
        s
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.dst_to_src.len()
    }

    #[inline(always)]
    pub fn to_map(&self, entity: Entity) -> MappedEntity {
        *self.src_to_dst.get(&entity).unwrap()
    }

    #[inline(always)]
    pub fn from_map(&self, mapped: MappedEntity) -> Entity {
        *self.dst_to_src.get(mapped.0 as usize).unwrap()
    }
}
