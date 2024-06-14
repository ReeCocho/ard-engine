pub mod storage;

use std::{any::TypeId, collections::HashMap, fmt::Debug};

use crate::{
    archetype::storage::{AnyArchetypeStorage, ArchetypeStorage},
    component::Component,
    entity::Entity,
    id_map::TypeIdMap,
    key::TypeKey,
    prelude::ComponentExt,
};

/// An archetype represents a logical set of components.
#[derive(Debug)]
pub struct Archetype {
    pub type_key: TypeKey,
    /// Maps the type id of the components in the archetype to the indices of the component vectors
    /// inside the associated `ArchetypeStorage` object.
    pub map: TypeIdMap<usize>,
    /// Index for the entity buffer within the entity storage.
    pub entities: usize,
}

/// Unique ID for an archetype.
#[derive(Debug, Copy, Clone, Default)]
pub struct ArchetypeId(u32);

/// Unique ID for an archetype storage.
#[derive(Debug, Copy, Clone, Default)]
pub struct ArchetypeStorageId(u32);

/// Holds a collection of archetypes.
#[derive(Default)]
pub struct Archetypes {
    /// All archetypes.
    archetypes: Vec<Archetype>,
    /// Maps archetype type keys to their unique ID.
    to_archetype: HashMap<TypeKey, ArchetypeId>,
    /// All archetype storages.
    storages: Vec<Box<dyn AnyArchetypeStorage>>,
    /// Makes component type ids to their storage ID.
    to_storage: TypeIdMap<ArchetypeStorageId>,
    /// Entity storages.
    entities: ArchetypeStorage<Entity>,
}

impl Archetypes {
    pub fn new() -> Self {
        Archetypes {
            archetypes: Vec::default(),
            to_archetype: HashMap::default(),
            storages: Vec::default(),
            to_storage: HashMap::default(),
            entities: ArchetypeStorage::new(),
        }
    }

    /// Gets a list of all archetypes.
    #[inline]
    pub fn archetypes(&self) -> &[Archetype] {
        &self.archetypes
    }

    /// Adds a new archetype and returns the ID of the archetype.
    ///
    /// # Note
    /// You can technically provide an invalid archetype here, but... Uh... Don't? :)
    #[inline]
    pub fn add_archetype(&mut self, archetype: Archetype) -> ArchetypeId {
        let id = ArchetypeId::from(self.archetypes.len());
        self.to_archetype.insert(archetype.type_key.clone(), id);
        self.archetypes.push(archetype);
        id
    }

    /// Gets a reference to an archetype and its ID by it's type key.
    ///
    /// Returns `None` if an archetype matching the type key doesn't exist.
    #[inline]
    pub fn get_archetype(&self, type_key: &TypeKey) -> Option<(&Archetype, ArchetypeId)> {
        self.to_archetype
            .get(type_key)
            .map(|i| (&self.archetypes[usize::from(*i)], *i))
    }

    /// Gets a mutable reference to an archetype and its index by it's type key.
    ///
    /// Returns `None` if an archetype matching the type key doesn't exist.
    #[inline]
    pub fn get_archetype_mut(
        &mut self,
        type_key: &TypeKey,
    ) -> Option<(&mut Archetype, ArchetypeId)> {
        if let Some(i) = self.to_archetype.get(type_key) {
            Some((&mut self.archetypes[usize::from(*i)], *i))
        } else {
            None
        }
    }

    #[inline]
    pub fn get_entity_storage(&self) -> &ArchetypeStorage<Entity> {
        &self.entities
    }

    #[inline]
    pub fn get_entity_storage_mut(&mut self) -> &mut ArchetypeStorage<Entity> {
        &mut self.entities
    }

    /// Gets the storage of a component.
    ///
    /// Returns `None` if a storage for the component type doesn't exist.
    #[inline]
    pub fn get_storage<T: Component + 'static>(&self) -> Option<&ArchetypeStorage<T>> {
        if let Some(i) = self.to_storage.get(&TypeId::of::<T>()) {
            Some(
                self.storages[usize::from(*i)]
                    .as_any()
                    .downcast_ref::<ArchetypeStorage<T>>()
                    .expect("Mismatched storage type"),
            )
        } else {
            None
        }
    }

    /// Gets mutable access to the storage of a component.
    ///
    /// Returns `None` if a storage for the component type doesn't exist.
    #[inline]
    pub fn get_storage_mut<T: Component + 'static>(&mut self) -> Option<&mut ArchetypeStorage<T>> {
        if let Some(i) = self.to_storage.get(&TypeId::of::<T>()) {
            Some(
                self.storages[usize::from(*i)]
                    .as_any_mut()
                    .downcast_mut::<ArchetypeStorage<T>>()
                    .expect("Mismatched storage type"),
            )
        } else {
            None
        }
    }

    /// Creates a new storage. Does nothing if the storage already exists.
    #[inline]
    pub fn create_storage<T: Component + 'static>(&mut self) {
        // Check if the storage already exists
        if self.to_storage.contains_key(&TypeId::of::<T>()) {
            return;
        }

        // Otherwise make the storage and mapping
        self.storages.push(Box::new(ArchetypeStorage::<T>::new()));
        self.to_storage.insert(
            TypeId::of::<T>(),
            ArchetypeStorageId::from(self.storages.len() - 1),
        );
    }

    /// Adds a component to an entity. The entity that was moved from the original archetype to
    /// fill its place is returned if there was one, along with the new archetype ID and index of
    /// the entity. If the entity already contained the component, the component is replaced and
    /// `None` is returned.
    pub fn add_component(
        &mut self,
        entity_id: Entity,
        src_archetype_id: ArchetypeId,
        index: usize,
        component: Box<dyn ComponentExt>,
    ) -> Option<(ArchetypeId, usize, Option<Entity>)> {
        // Grab the src archetype
        let component_id = component.type_id();
        let src_archetype = &mut self.archetypes[usize::from(src_archetype_id)];
        let src_entities = src_archetype.entities;

        // Determine which archetype the entity needs to be moved to, or create one
        let mut key = src_archetype.type_key.clone();
        if key.add_by_id(component_id) {
            // Component exists on entity already. Replace it.
            self.storages[usize::from(*self.to_storage.get(&component_id).unwrap())].replace(
                component.into_any(),
                *src_archetype.map.get(&component_id).unwrap(),
                index,
            );
            return None;
        }

        let dst_archetype_id = if let Some(id) = self.to_archetype.get(&key) {
            *id
        } else {
            let mut new_archetype = Archetype {
                type_key: key,
                map: HashMap::default(),
                entities: 0,
            };

            // All storages in the original archetype already exist. The new component type storage
            // might not exist, so we must check
            for ty in new_archetype.type_key.iter() {
                let storage = if *ty == component_id {
                    if let Some(id) = self.to_storage.get(ty) {
                        &mut self.storages[usize::from(*id)]
                    } else {
                        self.storages.push(component.create_storage());
                        let storage_id = ArchetypeStorageId(self.storages.len() as u32 - 1);
                        self.to_storage.insert(component_id, storage_id);
                        &mut self.storages[usize::from(storage_id)]
                    }
                } else {
                    &mut self.storages[usize::from(*self.to_storage.get(ty).unwrap())]
                };
                new_archetype.map.insert(*ty, storage.create());
            }
            new_archetype.entities = self.entities.create();

            self.add_archetype(new_archetype)
        };

        let dst_archetype = &mut self.archetypes[usize::from(dst_archetype_id)];

        // Remove from source entity list
        let mut entities = self.entities.get_mut(src_entities);

        // Entity is the last entity, so we don't need to worry about moving anything
        let moved_entity = if entities.len() - 1 == index {
            entities.pop();
            None
        }
        // Entity is not the last entity, so we need to determine which entity is moved
        else {
            let e = Some(*entities.last().unwrap());
            entities.swap_remove(index);
            e
        };

        // Add to destination entity list
        let mut entities = self.entities.get_mut(dst_archetype.entities);
        entities.push(entity_id);
        let new_idx = entities.len() - 1;
        let dst_archetype = &self.archetypes[usize::from(dst_archetype_id)];

        // Swap components from src buffers to dst buffers
        let src_archetype = &self.archetypes[usize::from(src_archetype_id)];
        for (id, src_buffer) in &src_archetype.map {
            if *id != component_id {
                let storage = &mut self.storages[usize::from(*self.to_storage.get(id).unwrap())];
                storage.swap_move(*dst_archetype.map.get(id).unwrap(), *src_buffer, index);
            }
        }

        // Add component to new buffer
        self.storages[usize::from(*self.to_storage.get(&component_id).unwrap())].add(
            component.into_any(),
            *dst_archetype.map.get(&component_id).unwrap(),
        );

        Some((dst_archetype_id, new_idx, moved_entity))
    }

    /// Removes a component from an entity. The entity that was moved from the original
    /// archetype to fill its place is returned if there was one, along with the new
    /// archetype ID and index of the entity. Returns `None` if the entity did not have the
    /// requested component type.
    pub fn remove_component(
        &mut self,
        entity_id: Entity,
        src_archetype_id: ArchetypeId,
        index: usize,
        component: TypeId,
    ) -> Option<(ArchetypeId, usize, Option<Entity>)> {
        // TODO: I wrote this while sleep deprived. Ripe for optimization, probably.

        // Grab the src archetype
        let src_archetype = &mut self.archetypes[usize::from(src_archetype_id)];
        let src_entities = src_archetype.entities;

        // Determine which archetype the entity needs to be moved to, or create one
        let mut key = src_archetype.type_key.clone();
        if !key.remove_by_id(component) {
            return None;
        }

        let dst_archetype_id = if let Some(id) = self.to_archetype.get(&key) {
            *id
        } else {
            let mut new_archetype = Archetype {
                type_key: key,
                map: HashMap::default(),
                entities: 0,
            };

            // All storages already exist since the entity is contained in them
            for ty in new_archetype.type_key.iter() {
                let storage = &mut self.storages[self.to_storage.get(ty).unwrap().0 as usize];
                new_archetype.map.insert(*ty, storage.create());
            }
            new_archetype.entities = self.entities.create();

            self.add_archetype(new_archetype)
        };

        let dst_archetype = &mut self.archetypes[usize::from(dst_archetype_id)];

        // Remove from source entity list
        let mut entities = self.entities.get_mut(src_entities);

        // Entity is the last entity, so we don't need to worry about moving anything
        let moved_entity = if entities.len() - 1 == index {
            entities.pop();
            None
        }
        // Entity is not the last entity, so we need to determine which entity is moved
        else {
            let e = Some(*entities.last().unwrap());
            entities.swap_remove(index);
            e
        };

        // Add to destination entity list
        let mut entities = self.entities.get_mut(dst_archetype.entities);
        entities.push(entity_id);
        let new_idx = entities.len() - 1;
        let dst_archetype = &self.archetypes[usize::from(dst_archetype_id)];

        // Swap components from src buffers to dst buffers
        let src_archetype = &self.archetypes[usize::from(src_archetype_id)];
        for (id, src_buffer) in &src_archetype.map {
            let storage = &mut self.storages[usize::from(*self.to_storage.get(id).unwrap())];
            if *id == component {
                storage.swap_remove(*src_buffer, index);
            } else {
                storage.swap_move(*dst_archetype.map.get(id).unwrap(), *src_buffer, index);
            }
        }

        Some((dst_archetype_id, new_idx, moved_entity))
    }

    /// Removes an entity from it's archetype by the index of the entity within the archetype.
    /// If an entity was moved to fill in the place of the removed entity, the moved entities
    /// handle is returned.
    ///
    /// # Panics
    /// Panics if the archetype ID is invalid, if the provided index is out of bounds in the
    /// archetype, or if the entities or buffers belonging to the components are currently
    /// requested for read or write.
    pub fn remove_entity(&mut self, archetype: ArchetypeId, index: usize) -> Option<Entity> {
        // Grab the archetype
        let archetype = &mut self.archetypes[usize::from(archetype)];

        // Aquire the entities and remove it
        let mut entities = self.entities.get_mut(archetype.entities);

        // Entity is the last entity, so we don't need to worry about moving components
        let ret = if entities.len() - 1 == index {
            entities.pop();
            None
        }
        // Entity is not the last entity, so we need to determine which entity is moved
        else {
            let e = Some(
                *entities
                    .last()
                    .expect("Attempt to get entity in empty buffer"),
            );
            entities.swap_remove(index);
            e
        };

        // For every component type in the archetype, remove the entities component and swap it
        // with the last component in the buffer
        for (id, buffer_index) in &archetype.map {
            self.storages[usize::from(
                *self
                    .to_storage
                    .get(id)
                    .expect("Archetype contains invalid storage"),
            )]
            .swap_remove(*buffer_index, index);
        }

        ret
    }
}

impl From<u32> for ArchetypeId {
    #[inline]
    fn from(item: u32) -> Self {
        ArchetypeId(item)
    }
}

impl From<usize> for ArchetypeId {
    #[inline]
    fn from(item: usize) -> Self {
        ArchetypeId(item as u32)
    }
}

impl From<ArchetypeId> for u32 {
    #[inline]
    fn from(item: ArchetypeId) -> Self {
        item.0
    }
}

impl From<ArchetypeId> for usize {
    #[inline]
    fn from(item: ArchetypeId) -> Self {
        item.0 as usize
    }
}

impl From<u32> for ArchetypeStorageId {
    #[inline]
    fn from(item: u32) -> Self {
        ArchetypeStorageId(item)
    }
}

impl From<usize> for ArchetypeStorageId {
    #[inline]
    fn from(item: usize) -> Self {
        ArchetypeStorageId(item as u32)
    }
}

impl From<ArchetypeStorageId> for u32 {
    #[inline]
    fn from(item: ArchetypeStorageId) -> Self {
        item.0
    }
}

impl From<ArchetypeStorageId> for usize {
    #[inline]
    fn from(item: ArchetypeStorageId) -> Self {
        item.0 as usize
    }
}
