pub mod entities;

use crate::{archetype::Archetypes, tag::Tags, world::entities::Entities};

/// A world contains the data of the ECS. It is used to create entities and add and remove
/// components from those entities.
#[derive(Default)]
pub struct World {
    /// Entities belonging to the world.
    pub(crate) entities: Entities,
    /// Archetypes and their storages.
    pub(crate) archetypes: Archetypes,
    /// Tags and their storages.
    pub(crate) tags: Tags,
}

impl World {
    pub fn new() -> World {
        World::default()
    }

    #[inline]
    pub fn entities(&self) -> &Entities {
        &self.entities
    }

    #[inline]
    pub fn entities_mut(&mut self) -> &mut Entities {
        &mut self.entities
    }

    #[inline]
    pub fn archetypes(&self) -> &Archetypes {
        &self.archetypes
    }

    #[inline]
    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    /// Processes all pending entities.
    pub fn process_entities(&mut self) {
        self.entities.process(&mut self.archetypes, &mut self.tags);
    }
}
