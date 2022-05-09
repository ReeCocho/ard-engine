pub mod access;
pub mod set;

use std::{any::Any, collections::HashMap};

use crate::{entity::Entity, prw_lock::PrwLock, tag::Tag};

pub trait TagStorage<T: Tag + Sized>: Default + TagStorageExt {
    /// Adds a new tag to the storage given a certain entity.
    fn add(&mut self, entity: Entity, tag: T);

    /// Get a reference to a tag from within the storage given the entity associated with it.
    /// Returns `None` if the tag is not associated with the entity.
    fn get(&self, entity: Entity) -> Option<&T>;

    /// Get a mutable reference to a tag from within the storage given the entity associated with
    /// it. Returns `None` if the tag is not associated with the entity.
    fn get_mut(&mut self, entity: Entity) -> Option<&mut T>;
}

pub trait TagStorageExt: Send + Sync {
    /// Adds a tag to the storage.
    fn add_dyn(&mut self, entity: Entity, tag: Box<dyn Any>);

    /// Removes tags from the storage belonging to the given entities.
    ///
    /// Silently fails if a tag for a given entity doesn't exist.
    fn remove(&mut self, entities: &[Entity]);
}

/// Trait implemented by RwLock<TagStorage> so there can be an interface for destroying tags
/// belonging to entities without know the tags type.
pub trait AnyTagStorage: Send + Sync {
    /// Converts the type into an any reference.
    fn as_any(&self) -> &dyn Any;

    /// Converts the type into a mutable any reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Adds a tag to the storage.
    fn add_dyn(&mut self, entity: Entity, tag: Box<dyn Any>);

    /// Removes the tags belonging to the given entities.
    ///
    /// # Panics
    /// Panics if the storage is already requested for read/write.
    fn remove(&self, entities: &[Entity]);
}

/// Holds components that use `CommonStorage`.
pub struct CommonStorage<T: Tag> {
    /// Container of components.
    components: Vec<Option<T>>,
    /// Mapping of entities to components.
    entities: Vec<Entity>,
}

/// Holds components that use `UncommonStorage`.
pub struct UncommonStorage<T: Tag> {
    /// Container of components.
    components: HashMap<Entity, T>,
}

impl<T: TagStorageExt + 'static> AnyTagStorage for PrwLock<T> {
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline]
    fn add_dyn(&mut self, entity: Entity, tag: Box<dyn Any>) {
        self.write().add_dyn(entity, tag);
    }

    #[inline]
    fn remove(&self, entities: &[Entity]) {
        self.write().remove(entities);
    }
}

impl<T: Tag> Default for CommonStorage<T> {
    #[inline]
    fn default() -> Self {
        Self {
            components: Vec::default(),
            entities: Vec::default(),
        }
    }
}

impl<T: Tag + 'static> TagStorage<T> for CommonStorage<T> {
    #[inline]
    fn add(&mut self, entity: Entity, tag: T) {
        // Compute tag index
        let ind = entity.id() as usize;

        // Resize tag buffer if needed
        if ind >= self.entities.len() {
            let new_size = ind + 1;
            self.entities.resize(new_size, Entity::null());
            self.components.resize_with(new_size, Option::default);
        }

        // Add the tag
        self.components[ind] = Some(tag);
        self.entities[ind] = entity;
    }

    #[inline]
    fn get(&self, entity: Entity) -> Option<&T> {
        if (entity.id() as usize) < self.entities.len() {
            if let Some(component) = &self.components[entity.id() as usize] {
                return Some(component);
            }
        }

        None
    }

    #[inline]
    fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        if (entity.id() as usize) < self.entities.len() {
            if let Some(component) = &mut self.components[entity.id() as usize] {
                return Some(component);
            }
        }

        None
    }
}

impl<T: Tag + 'static> TagStorageExt for CommonStorage<T> {
    #[inline]
    fn add_dyn(&mut self, entity: Entity, tag: Box<dyn Any>) {
        self.add(
            entity,
            *tag.downcast::<T>()
                .expect("invalid tag type provided to common storage for add"),
        );
    }

    #[inline]
    fn remove(&mut self, entities: &[Entity]) {
        for entity in entities {
            // Only works if in range
            if (entity.id() as usize) < self.entities.len() {
                let ind = entity.id() as usize;
                self.components[ind] = None;
                self.entities[ind] = Entity::null();
            }
        }
    }
}

impl<T: Tag> Default for UncommonStorage<T> {
    fn default() -> Self {
        Self {
            components: HashMap::default(),
        }
    }
}

impl<T: Tag + 'static> TagStorage<T> for UncommonStorage<T> {
    #[inline]
    fn add(&mut self, entity: Entity, tag: T) {
        self.components.insert(entity, tag);
    }

    #[inline]
    fn get(&self, entity: Entity) -> Option<&T> {
        self.components.get(&entity)
    }

    #[inline]
    fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.components.get_mut(&entity)
    }
}

impl<T: Tag + 'static> TagStorageExt for UncommonStorage<T> {
    #[inline]
    fn add_dyn(&mut self, entity: Entity, tag: Box<dyn Any>) {
        self.add(
            entity,
            *tag.downcast::<T>()
                .expect("invalid tag type provided to common storage for add"),
        );
    }

    #[inline]
    fn remove(&mut self, entities: &[Entity]) {
        for entity in entities {
            self.components.remove(entity);
        }
    }
}
