use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::{storage::set::ArchetypeStorageSet, Archetypes},
    component::filter::ComponentFilter,
    entity::Entity,
    key::TypeKey,
    prelude::{Component, Entities},
    prw_lock::PrwReadLock,
    system::data::SystemData,
    tag::{filter::TagFilter, storage::set::TagStorageSet, Tags},
};

/// An object used to create queries.
pub struct Queries<S: SystemData> {
    entities: NonNull<Entities>,
    tags: NonNull<Tags>,
    archetypes: NonNull<Archetypes>,
    mut_components: TypeKey,
    all_components: TypeKey,
    mut_tags: TypeKey,
    all_tags: TypeKey,
    _phantom: std::marker::PhantomData<S>,
}

pub struct QueryFilter<'a, S: SystemData> {
    queries: &'a Queries<S>,
    without: TypeKey,
    with: TypeKey,
}

/// A query is a request by a system for access to the system required components and tags.
pub trait Query<C: ComponentFilter, T: TagFilter> {
    /// Creates a new instance of the query.
    fn new(tags: &Tags, archetypes: &Archetypes, with: TypeKey, without: TypeKey) -> Self;

    fn is_empty(&self) -> bool;

    /// Number of entities found using this query.
    fn len(&self) -> usize;
}

pub trait SingleQuery<C: ComponentFilter, T: TagFilter>: Sized {
    fn make(
        entity: Entity,
        tags: &Tags,
        archetypes: &Archetypes,
        entities: &Entities,
    ) -> Option<Self>;
}

/// A query that only requests components.
pub struct ComponentQuery<Components: ComponentFilter> {
    /// List of storage sets to loop over.
    sets: Vec<Components::StorageSet>,
    /// Current working set.
    set: Option<Components::StorageSet>,
    /// Current working set index.
    idx: usize,
    len: usize,
}

pub struct SingleComponentQuery<Components: ComponentFilter> {
    _set: Components::StorageSet,
    data: <Components::StorageSet as ArchetypeStorageSet>::Filter,
}

pub struct SingleTagQuery<Tags: TagFilter> {
    _set: Tags::StorageSet,
    data: <Tags::StorageSet as TagStorageSet>::TagSet,
}

/// A query that only requests components and their associated entity.
pub struct EntityComponentQuery<Components: ComponentFilter> {
    /// List of storage sets and entity buffers to loop over.
    sets: Vec<(FastEntityIterator, Components::StorageSet)>,
    /// Current working set and entity buffer.
    set: Option<(FastEntityIterator, Components::StorageSet)>,
    /// Current working set index.
    idx: usize,
    len: usize,
}

/// A query that only requests components, tags, and their associated entity.
pub struct EntityComponentTagQuery<Components: ComponentFilter, Tags: TagFilter> {
    /// List of storage sets and entity buffers to loop over.
    sets: Vec<(FastEntityIterator, Components::StorageSet)>,
    /// Current working set and entity buffer.
    set: Option<(FastEntityIterator, Components::StorageSet)>,
    /// Tag storages to poll using.
    tags: Tags::StorageSet,
    /// Current working set index.
    idx: usize,
    len: usize,
}

pub struct SingleComponentTagQuery<Components: ComponentFilter, Tags: TagFilter> {
    _comp_set: Components::StorageSet,
    _tag_set: Tags::StorageSet,
    data: (
        <Components::StorageSet as ArchetypeStorageSet>::Filter,
        <Tags::StorageSet as TagStorageSet>::TagSet,
    ),
}

/// Special fast iterator for entity storages.
struct FastEntityIterator {
    #[allow(dead_code)]
    handle: Option<PrwReadLock<Vec<Entity>>>,
    ptr: NonNull<Entity>,
}

impl<S: SystemData> Queries<S> {
    pub fn new(tags: &Tags, archetypes: &Archetypes, entities: &Entities) -> Self {
        let all_components = S::Components::type_key();
        let mut_components = S::Components::mut_type_key();
        let all_tags = S::Tags::type_key();
        let mut_tags = S::Tags::mut_type_key();

        Self {
            tags: unsafe { NonNull::new_unchecked(tags as *const _ as *mut _) },
            archetypes: unsafe { NonNull::new_unchecked(archetypes as *const _ as *mut _) },
            entities: unsafe { NonNull::new_unchecked(entities as *const _ as *mut _) },
            all_components,
            mut_components,
            all_tags,
            mut_tags,
            _phantom: Default::default(),
        }
    }

    /// Allows for finer filtering of entities in a query.
    #[inline]
    pub fn filter(&self) -> QueryFilter<S> {
        QueryFilter {
            queries: self,
            without: TypeKey::default(),
            with: TypeKey::default(),
        }
    }

    /// Creates a new query.
    #[inline]
    pub fn make<T: SystemData>(&self) -> T::Query {
        self.filter().make::<T>()
    }

    /// Queries a single entity for components and tags. Returns `None` if any one of the
    /// components is not contained within the entity.
    pub fn get<T: SystemData>(&self, entity: Entity) -> Option<T::SingleQuery> {
        // No need to perform checks if we have access to everything
        if !S::EVERYTHING {
            let read_components = T::Components::read_type_key();
            let mut_components = T::Components::mut_type_key();

            let read_tags = T::Tags::read_type_key();
            let mut_tags = T::Tags::mut_type_key();

            // Reads must be a subset of all (it is ok to request read when originally requesting write)
            debug_assert!(read_components.subset_of(&self.all_components));
            debug_assert!(read_tags.subset_of(&self.all_tags));

            // Writes must be a subset of writes (can't request read first and then write later)
            debug_assert!(mut_components.subset_of(&self.mut_components));
            debug_assert!(mut_tags.subset_of(&self.mut_tags));
        }

        unsafe {
            T::SingleQuery::make(
                entity,
                self.tags.as_ref(),
                self.archetypes.as_ref(),
                self.entities.as_ref(),
            )
        }
    }

    /// Queries a single entity for tags.
    pub fn get_tag<T: TagFilter>(&self, entity: Entity) -> SingleTagQuery<T> {
        // No need to perform checks if we have access to everything
        if !S::EVERYTHING {
            let read_tags = T::read_type_key();
            let mut_tags = T::mut_type_key();

            debug_assert!(read_tags.subset_of(&self.all_tags));
            debug_assert!(mut_tags.subset_of(&self.mut_tags));
        }

        unsafe { SingleTagQuery::make(self.tags.as_ref(), entity) }
    }
}

impl<'a, S: SystemData> QueryFilter<'a, S> {
    #[inline]
    pub fn with<C: Component + 'static>(mut self) -> Self {
        self.with.add::<C>();
        self
    }

    #[inline]
    pub fn without<C: Component + 'static>(mut self) -> Self {
        self.without.add::<C>();
        self
    }

    #[inline]
    pub fn make<T: SystemData>(self) -> T::Query {
        // No need to perform checks if we have access to everything
        if !S::EVERYTHING {
            let read_components = T::Components::read_type_key();
            let mut_components = T::Components::mut_type_key();

            let read_tags = T::Tags::read_type_key();
            let mut_tags = T::Tags::mut_type_key();

            // Reads must be a subset of all (it is ok to request read when originally requesting write)
            debug_assert!(read_components.subset_of(&self.queries.all_components));
            debug_assert!(read_tags.subset_of(&self.queries.all_tags));

            // Writes must be a subset of writes (can't request read first and then write later)
            debug_assert!(mut_components.subset_of(&self.queries.mut_components));
            debug_assert!(mut_tags.subset_of(&self.queries.mut_tags));
        }

        unsafe {
            T::Query::new(
                self.queries.tags.as_ref(),
                self.queries.archetypes.as_ref(),
                self.with,
                self.without,
            )
        }
    }
}

impl<C: ComponentFilter, T: TagFilter> Query<C, T> for () {
    fn new(_: &Tags, _: &Archetypes, _: TypeKey, _: TypeKey) -> Self {}

    #[inline]
    fn is_empty(&self) -> bool {
        true
    }

    #[inline]
    fn len(&self) -> usize {
        0
    }
}

impl<C: ComponentFilter, T: TagFilter> SingleQuery<C, T> for () {
    fn make(_: Entity, _: &Tags, _: &Archetypes, _: &Entities) -> Option<Self> {
        None
    }
}

impl<Components: ComponentFilter> Query<Components, ()> for EntityComponentQuery<Components> {
    fn new(_: &Tags, archetypes: &Archetypes, with: TypeKey, without: TypeKey) -> Self {
        // Generate the descriptor for the filter
        let descriptor = Components::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them and their corresponding entites.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
            // Must not have any of without
            if !archetype.type_key.none_of(&without) {
                continue;
            }

            // Must have all of with
            if !with.subset_of(&archetype.type_key) {
                continue;
            }

            // Must be compatible
            if descriptor.subset_of(&archetype.type_key) {
                // Grab entity storage
                let handle = archetypes.get_entity_storage().get(archetype.entities);

                // Must have non-zero entity count
                if handle.len() != 0 {
                    len += handle.len();

                    // Add set and entity buffer
                    sets.push((
                        FastEntityIterator::new(handle),
                        Components::make_storage_set(archetype, archetypes).unwrap(),
                    ));
                }
            }
        }

        // Grab the starting set and entity buffer
        let set = sets.pop();

        Self {
            sets,
            set,
            idx: 0,
            len,
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl<Components: ComponentFilter> Iterator for EntityComponentQuery<Components> {
    type Item = (
        Entity,
        <Components::StorageSet as ArchetypeStorageSet>::Filter,
    );

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have a working set
        if let Some((entities, set)) = &mut self.set {
            // Grab the filter and entity
            // NOTE: Safe to unwrap since sets are guaranteed not to be empty and if the set
            // wasn't valid last loop, it would have been replaced with a valid one.
            let filter = unsafe { set.fetch(self.idx) };
            let entity = unsafe { entities.fetch(self.idx) };

            // Move to the next set if the current is invalid
            self.idx += 1;
            if !set.is_valid(self.idx) {
                self.set = self.sets.pop();
                self.idx = 0;
            }

            Some((entity, filter))
        } else {
            None
        }
    }
}

impl<Components: ComponentFilter> Query<Components, ()> for ComponentQuery<Components> {
    fn new(_: &Tags, archetypes: &Archetypes, with: TypeKey, without: TypeKey) -> Self {
        // Generate the descriptor for the filter
        let descriptor = Components::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
            // Must not have any of without
            if !archetype.type_key.none_of(&without) {
                continue;
            }

            // Must have all of with
            if !with.subset_of(&archetype.type_key) {
                continue;
            }

            // Must be compatible
            if descriptor.subset_of(&archetype.type_key) {
                // Must have non-zero entity count
                let entity_count = archetypes
                    .get_entity_storage()
                    .get(archetype.entities)
                    .len();

                if entity_count != 0 {
                    len += entity_count;
                    sets.push(Components::make_storage_set(archetype, archetypes).unwrap());
                }
            }
        }

        // Grab the starting set
        let set = sets.pop();

        Self {
            sets,
            set,
            idx: 0,
            len,
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl<Components: ComponentFilter> Iterator for ComponentQuery<Components> {
    type Item = <Components::StorageSet as ArchetypeStorageSet>::Filter;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have a working set
        if let Some(set) = &mut self.set {
            // Grab the filter
            // NOTE: Safe to unwrap since sets are guaranteed not to be empty and if the set
            // wasn't valid last loop, it would have been replaced with a valid one.
            let filter = unsafe { set.fetch(self.idx) };

            // Move to the next set if the current is invalid
            self.idx += 1;
            if !set.is_valid(self.idx) {
                self.set = self.sets.pop();
                self.idx = 0;
            }

            Some(filter)
        } else {
            None
        }
    }
}

impl<C: ComponentFilter> SingleQuery<C, ()> for SingleComponentQuery<C> {
    fn make(
        entity: Entity,
        _: &Tags,
        archetypes: &Archetypes,
        entities: &Entities,
    ) -> Option<Self> {
        let (archetype, idx) = match entities.entities().get(entity.id() as usize) {
            Some(info) => {
                if info.ver.get() != entity.ver() {
                    return None;
                }

                (info.archetype, info.index)
            }
            None => return None,
        };

        C::make_storage_set(&archetypes.archetypes()[usize::from(archetype)], archetypes).map(
            |set| {
                let data = unsafe { set.fetch(idx as usize) };
                Self { _set: set, data }
            },
        )
    }
}

impl<C: ComponentFilter> Deref for SingleComponentQuery<C> {
    type Target = <C::StorageSet as ArchetypeStorageSet>::Filter;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<C: ComponentFilter> DerefMut for SingleComponentQuery<C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<C: ComponentFilter, T: TagFilter> Query<C, T> for EntityComponentTagQuery<C, T> {
    fn new(tags: &Tags, archetypes: &Archetypes, with: TypeKey, without: TypeKey) -> Self {
        // Generate the descriptor for the filter
        let descriptor = C::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them and their corresponding entites.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
            // Must not have any of without
            if !archetype.type_key.none_of(&without) {
                continue;
            }

            // Must have all of with
            if !with.subset_of(&archetype.type_key) {
                continue;
            }

            // Must be compatible
            if descriptor.subset_of(&archetype.type_key) {
                // Grab entity storage
                let handle = archetypes.get_entity_storage().get(archetype.entities);

                // Must have non-zero entity count
                if handle.len() != 0 {
                    len += handle.len();

                    // Add set and entity buffer
                    sets.push((
                        FastEntityIterator::new(handle),
                        C::make_storage_set(archetype, archetypes).unwrap(),
                    ));
                }
            }
        }

        // Grab the starting set and entity buffer
        let set = sets.pop();

        Self {
            sets,
            set,
            tags: T::StorageSet::from_tags(tags),
            idx: 0,
            len,
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl<C: ComponentFilter, T: TagFilter> Iterator for EntityComponentTagQuery<C, T> {
    type Item = (
        Entity,
        <C::StorageSet as ArchetypeStorageSet>::Filter,
        <T::StorageSet as TagStorageSet>::TagSet,
    );

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have a working set
        if let Some((entities, set)) = &mut self.set {
            // Grab the filter and entity
            // NOTE: Safe to unwrap since sets are guaranteed not to be empty and if the set
            // wasn't valid last loop, it would have been replaced with a valid one.
            let filter = unsafe { set.fetch(self.idx) };
            let entity = unsafe { entities.fetch(self.idx) };

            // Move to the next set if the current is invalid
            self.idx += 1;
            if !set.is_valid(self.idx) {
                self.set = self.sets.pop();
                self.idx = 0;
            }

            Some((entity, filter, self.tags.make_set(entity)))
        } else {
            None
        }
    }
}

impl<C: ComponentFilter, T: TagFilter> SingleQuery<C, T> for SingleComponentTagQuery<C, T> {
    fn make(
        entity: Entity,
        tags: &Tags,
        archetypes: &Archetypes,
        entities: &Entities,
    ) -> Option<Self> {
        let (archetype, idx) = match entities.entities().get(entity.id() as usize) {
            Some(info) => {
                if info.ver.get() != entity.ver() {
                    return None;
                }

                (info.archetype, info.index)
            }
            None => return None,
        };

        let set = C::make_storage_set(&archetypes.archetypes()[usize::from(archetype)], archetypes)
            .unwrap();
        let data = unsafe { set.fetch(idx as usize) };

        let tag_set = T::StorageSet::from_tags(tags);
        let tags = tag_set.make_set(entity);

        Some(Self {
            _comp_set: set,
            _tag_set: tag_set,
            data: (data, tags),
        })
    }
}

impl<C: ComponentFilter, T: TagFilter> Deref for SingleComponentTagQuery<C, T> {
    type Target = (
        <C::StorageSet as ArchetypeStorageSet>::Filter,
        <T::StorageSet as TagStorageSet>::TagSet,
    );

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<C: ComponentFilter, T: TagFilter> DerefMut for SingleComponentTagQuery<C, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: TagFilter> SingleTagQuery<T> {
    fn make(tags: &Tags, entity: Entity) -> Self {
        let set = T::StorageSet::from_tags(tags);
        let data = set.make_set(entity);

        Self { _set: set, data }
    }
}

impl<T: TagFilter> Deref for SingleTagQuery<T> {
    type Target = <T::StorageSet as TagStorageSet>::TagSet;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: TagFilter> DerefMut for SingleTagQuery<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl Default for FastEntityIterator {
    #[inline]
    fn default() -> Self {
        Self {
            handle: None,
            ptr: NonNull::new(1 as *mut Entity).unwrap(),
        }
    }
}

impl FastEntityIterator {
    #[inline]
    fn new(handle: PrwReadLock<Vec<Entity>>) -> Self {
        debug_assert!(handle.len() != 0);

        // Cast const to mut, but we never modify the buffer so it's totally cool
        let ptr = handle.as_ptr() as *mut Entity;
        FastEntityIterator {
            handle: Some(handle),
            // Safe to unwrap since len != 0, which means the buffer must be allocated
            ptr: NonNull::new(ptr).expect("Empty lock given to fast entity iterator"),
        }
    }

    #[inline]
    unsafe fn fetch(&mut self, idx: usize) -> Entity {
        *self.ptr.as_ptr().add(idx)
    }
}

unsafe impl Send for FastEntityIterator {}
