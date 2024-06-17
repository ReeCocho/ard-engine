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
    pub fn mapped(&self) -> &[Entity] {
        &self.dst_to_src
    }

    #[inline(always)]
    pub fn to_map_or_insert(&mut self, entity: Entity) -> MappedEntity {
        *self.src_to_dst.entry(entity).or_insert_with(|| {
            let new_id = MappedEntity(self.dst_to_src.len() as u32);
            self.dst_to_src.push(entity);
            new_id
        })
    }

    #[inline(always)]
    pub fn from_map_or_null(&self, mapped: MappedEntity) -> Entity {
        self.dst_to_src
            .get(mapped.0 as usize)
            .cloned()
            .unwrap_or(Entity::null())
    }
}
