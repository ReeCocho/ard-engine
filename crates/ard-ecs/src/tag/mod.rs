pub mod access;
pub mod filter;
pub mod pack;
pub mod storage;

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    num::NonZeroU32,
};

use crate::{
    entity::Entity,
    id_map::TypeIdMap,
    key::TypeKey,
    prw_lock::{PrwLock, PrwReadLock, PrwWriteLock},
    tag::storage::{AnyTagStorage, TagStorage},
};

pub use ard_ecs_derive::Tag;

/// A tag is like a component, except they can be added and removed from entities quickly.
pub trait Tag: Sized + Send + Sync {
    const NAME: &'static str;

    /// Type of storage used to hold components.
    type Storage: TagStorage<Self>;
}

pub trait TagExt: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn into_any(self: Box<Self>) -> Box<dyn Any>;

    /// Make the storage type for this tag.
    fn make_storage(&self) -> Box<dyn AnyTagStorage>;
}

/// Holds tag storages.
#[derive(Default)]
pub struct Tags {
    /// All storages.
    storages: Vec<Box<dyn AnyTagStorage>>,
    /// All tag collections
    collections: Vec<TagCollection>,
    /// Maps tag IDs to the storages containing them.
    to_storage: TypeIdMap<StorageId>,
    /// Maps type sets of tags to the index of their collection.
    to_collection: HashMap<TypeKey, TagCollectionId>,
}

/// A tag collection represents a logical set of tags.
#[derive(Debug)]
pub struct TagCollection {
    pub type_key: TypeKey,
}

/// Unique storage ID.
#[derive(Debug, Copy, Clone, Default)]
pub struct StorageId(u32);

/// Unique tag collection ID.
///
/// # Note
/// Internally, the ID is a `NonZeroU32`. This is because the `EntityInfo` object contained in
/// `Entities` has an `Option<TagCollectionId>`. With a `NonZeroU32`, we get an optional version
/// for free (in terms of space).
#[derive(Debug, Copy, Clone)]
pub struct TagCollectionId(NonZeroU32);

impl Tags {
    pub fn new() -> Tags {
        Tags::default()
    }

    /// Attempts to get write access to a storage tag type. If the storage hasn't been created yet,
    /// a new one is created.
    ///
    /// # Panics
    ///
    /// Panics if the storage is currently requested for reading or writing.
    pub fn get_storage_or_default_mut<T: Tag + 'static>(&mut self) -> PrwWriteLock<T::Storage> {
        // See if the storage already exists
        let id = if let Some(id) = self.to_storage.get(&TypeId::of::<T>()) {
            *id
        }
        // Storage doesn't exist. We need to make it.
        else {
            let id = StorageId::from(self.storages.len());
            self.storages
                .push(Box::new(PrwLock::new(T::Storage::default())));
            self.to_storage.insert(TypeId::of::<T>(), id);
            id
        };

        self.storages[usize::from(id)]
            .as_any()
            .downcast_ref::<PrwLock<T::Storage>>()
            .expect("Tag associated with incorrect storage type.")
            .write()
    }

    /// Attempts to get read access to the storage of a tag type. Returns `None` if the storage
    /// doesn't exist.
    ///
    /// # Panics
    ///
    /// Panics if the storage is currently requested for writing.
    pub fn get_storage<T: Tag + 'static>(&self) -> Option<PrwReadLock<T::Storage>> {
        self.to_storage.get(&TypeId::of::<T>()).map(|id| {
            self.storages[usize::from(*id)]
                .as_any()
                .downcast_ref::<PrwLock<T::Storage>>()
                .expect("Tag associated with incorrect storage type.")
                .read()
        })
    }

    /// Attempts to get read/write access to the storage of a tag type. Returns `None` if the storage
    /// doesn't exist.
    ///
    /// # Panics
    ///
    /// Panics if the storage is currently requested for reading or writing.
    pub fn get_storage_mut<T: Tag + 'static>(&self) -> Option<PrwWriteLock<T::Storage>> {
        self.to_storage.get(&TypeId::of::<T>()).map(|id| {
            self.storages[usize::from(*id)]
                .as_any()
                .downcast_ref::<PrwLock<T::Storage>>()
                .expect("Tag associated with incorrect storage type.")
                .write()
        })
    }

    /// Gets access to a storage by the type ID of the tag contained within. Returns `None` if the
    /// storage doesn't exist.
    #[inline]
    pub fn get_storage_by_tag_id(&self, id: TypeId) -> Option<&dyn AnyTagStorage> {
        if let Some(id) = self.to_storage.get(&id) {
            Some(self.storages[usize::from(*id)].as_ref())
        } else {
            None
        }
    }

    /// Gets access to a storage by its ID.
    ///
    /// # Panics
    /// Panics if an invalid ID is provided.
    #[inline]
    pub fn get_storage_by_id(&self, id: StorageId) -> &dyn AnyTagStorage {
        self.storages[usize::from(id)].as_ref()
    }

    /// Gets the ID of the storage by the tag contained in it. Returns `None` if the tag
    /// doesn't exist.
    #[inline]
    pub fn get_storage_id(&self, id: TypeId) -> Option<StorageId> {
        self.to_storage.get(&id).copied()
    }

    /// Adds a new tag collection and returns the ID of the collection.
    ///
    /// # Note
    /// You can technically provide an invalid collection here, but... Uh... Don't? :)
    #[inline]
    pub fn add_collection(&mut self, collection: TagCollection) -> TagCollectionId {
        let id = TagCollectionId::from(
            NonZeroU32::new((self.collections.len() + 1) as u32).expect("0 tag collection ID"),
        );
        self.to_collection.insert(collection.type_key.clone(), id);
        self.collections.push(collection);
        id
    }

    /// Gets a reference to an collection by its ID.
    ///
    /// Returns `None` if a collection matching the ID doesn't exist.
    #[inline]
    pub fn get_collection(&self, id: TagCollectionId) -> Option<&TagCollection> {
        let ind = usize::from(id) - 1;
        if ind < self.collections.len() {
            Some(&self.collections[ind])
        } else {
            None
        }
    }

    /// Gets the ID of a collection by it's type key.
    ///
    /// Returns `None` if a collection matching the type key doesn't exist.
    #[inline]
    pub fn get_collection_id(&self, type_key: &TypeKey) -> Option<TagCollectionId> {
        self.to_collection.get(type_key).copied()
    }

    /// Adds a tag to an entity, or replaces the existing one. Returns the new tag collection id
    /// of the entity.
    #[inline]
    pub fn add_tag(
        &mut self,
        entity: Entity,
        collection: Option<TagCollectionId>,
        tag: Box<dyn TagExt>,
    ) -> TagCollectionId {
        let tag_id = tag.as_ref().type_id();

        // Add the tag to the proper storage
        let storage = if let Some(id) = self.to_storage.get(&tag_id) {
            &mut self.storages[usize::from(*id)]
        } else {
            self.storages.push(tag.make_storage());
            self.to_storage
                .insert(tag_id, StorageId(self.storages.len() as u32 - 1));
            self.storages.last_mut().unwrap()
        };
        storage.add_dyn(entity, tag.into_any());

        // Generate the new type key for the entity
        let mut type_key = if let Some(collection) = collection {
            self.collections[usize::from(collection) - 1]
                .type_key
                .clone()
        } else {
            TypeKey::default()
        };
        type_key.add_by_id(tag_id);

        // Get the new tag collection id, or create one if needed
        if let Some(collection) = self.to_collection.get(&type_key) {
            *collection
        } else {
            self.add_collection(TagCollection { type_key })
        }
    }

    /// Removes a tag from the collection. Returns the new tag collection ID, or none if there were
    /// no more tags left.
    #[inline]
    pub fn remove_tag(
        &mut self,
        entity: Entity,
        tag_id: TypeId,
        collection: TagCollectionId,
    ) -> Option<TagCollectionId> {
        // Remove from storage
        if let Some(storage) = self.to_storage.get(&tag_id) {
            self.storages[usize::from(*storage)].remove(&[entity]);
        }

        // Compute new collection id
        let mut type_key = self.collections[usize::from(collection) - 1]
            .type_key
            .clone();
        type_key.remove_by_id(tag_id);

        // Get the new tag collection id, or create one if needed
        if type_key.is_empty() {
            None
        } else if let Some(collection) = self.to_collection.get(&type_key) {
            Some(*collection)
        } else {
            Some(self.add_collection(TagCollection { type_key }))
        }
    }

    pub fn remove_entity(&mut self, entity: Entity, collection: TagCollectionId) {
        self.collections[usize::from(collection)]
            .type_key
            .iter()
            .for_each(|tag_ty| {
                if let Some(storage) = self.to_storage.get(tag_ty) {
                    self.storages[usize::from(*storage)].remove(&[entity]);
                }
            });
    }
}

impl TagCollection {
    #[inline(always)]
    pub fn type_key(&self) -> &TypeKey {
        &self.type_key
    }
}

impl<T: Tag + 'static> TagExt for T {
    #[inline]
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    #[inline]
    fn into_any(self: Box<T>) -> Box<dyn Any> {
        self
    }

    #[inline]
    fn make_storage(&self) -> Box<dyn AnyTagStorage> {
        Box::new(PrwLock::new(T::Storage::default()))
    }
}

impl From<u32> for StorageId {
    #[inline]
    fn from(item: u32) -> Self {
        StorageId(item)
    }
}

impl From<usize> for StorageId {
    #[inline]
    fn from(item: usize) -> Self {
        StorageId(item as u32)
    }
}

impl From<StorageId> for u32 {
    #[inline]
    fn from(item: StorageId) -> Self {
        item.0
    }
}

impl From<StorageId> for usize {
    #[inline]
    fn from(item: StorageId) -> Self {
        item.0 as usize
    }
}

impl From<NonZeroU32> for TagCollectionId {
    #[inline]
    fn from(item: NonZeroU32) -> Self {
        TagCollectionId(item)
    }
}

impl From<TagCollectionId> for NonZeroU32 {
    #[inline]
    fn from(item: TagCollectionId) -> Self {
        item.0
    }
}

impl From<TagCollectionId> for usize {
    #[inline]
    fn from(item: TagCollectionId) -> Self {
        item.0.get() as usize
    }
}

impl Default for TagCollectionId {
    fn default() -> Self {
        TagCollectionId(NonZeroU32::new(1).unwrap())
    }
}
