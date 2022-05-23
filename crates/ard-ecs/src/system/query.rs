use std::ptr::NonNull;

use crate::{
    archetype::{storage::set::ArchetypeStorageSet, Archetypes},
    component::filter::ComponentFilter,
    entity::Entity,
    key::TypeKey,
    prw_lock::PrwReadLock,
    system::data::SystemData,
    tag::{filter::TagFilter, storage::set::TagStorageSet, Tags},
};

/// An object used to create queries.
pub struct Queries<S: SystemData> {
    tags: NonNull<Tags>,
    archetypes: NonNull<Archetypes>,
    mut_components: TypeKey,
    all_components: TypeKey,
    mut_tags: TypeKey,
    all_tags: TypeKey,
    _phantom: std::marker::PhantomData<S>,
}

/// A query is a request by a system for access to the system required components and tags.
pub trait Query<C: ComponentFilter, T: TagFilter> {
    /// Creates a new instance of the query.
    fn new(tags: &Tags, archetypes: &Archetypes) -> Self;

    fn is_empty(&self) -> bool;

    /// Number of entities found using this query.
    fn len(&self) -> usize;
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

/// Special fast iterator for entity storages.
struct FastEntityIterator {
    #[allow(dead_code)]
    handle: Option<PrwReadLock<Vec<Entity>>>,
    ptr: NonNull<Entity>,
}

impl<S: SystemData> Queries<S> {
    pub fn new(tags: &Tags, archetypes: &Archetypes) -> Self {
        let all_components = S::Components::type_key();
        let mut_components = S::Components::mut_type_key();
        let all_tags = S::Tags::type_key();
        let mut_tags = S::Tags::mut_type_key();

        Self {
            tags: unsafe { NonNull::new_unchecked(tags as *const _ as *mut _) },
            archetypes: unsafe { NonNull::new_unchecked(archetypes as *const _ as *mut _) },
            all_components,
            mut_components,
            all_tags,
            mut_tags,
            _phantom: Default::default(),
        }
    }

    /// Creates a new query.
    pub fn make<T: SystemData>(&self) -> T::Query {
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

        unsafe { T::Query::new(self.tags.as_ref(), self.archetypes.as_ref()) }
    }
}

impl<C: ComponentFilter, T: TagFilter> Query<C, T> for () {
    fn new(_: &Tags, _: &Archetypes) -> Self {}

    #[inline]
    fn is_empty(&self) -> bool {
        true
    }

    #[inline]
    fn len(&self) -> usize {
        0
    }
}

impl<Components: ComponentFilter> Query<Components, ()> for EntityComponentQuery<Components> {
    fn new(_: &Tags, archetypes: &Archetypes) -> Self {
        // Generate the descriptor for the filter
        let descriptor = Components::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them and their corresponding entites.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
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
                        Components::make_storage_set(archetype, archetypes),
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

impl<'a, Components: ComponentFilter> Iterator for EntityComponentQuery<Components> {
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
    fn new(_: &Tags, archetypes: &Archetypes) -> Self {
        // Generate the descriptor for the filter
        let descriptor = Components::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
            // Must be compatible
            if descriptor.subset_of(&archetype.type_key) {
                // Must have non-zero entity count
                let entity_count = archetypes
                    .get_entity_storage()
                    .get(archetype.entities)
                    .len();

                if entity_count != 0 {
                    len += entity_count;
                    sets.push(Components::make_storage_set(archetype, archetypes));
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

impl<'a, Components: ComponentFilter> Iterator for ComponentQuery<Components> {
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

impl<C: ComponentFilter, T: TagFilter> Query<C, T> for EntityComponentTagQuery<C, T> {
    fn new(tags: &Tags, archetypes: &Archetypes) -> Self {
        // Generate the descriptor for the filter
        let descriptor = C::type_key();

        let mut len = 0;

        // Find all archetypes that our descriptor is a subset of and generate storage sets with
        // them and their corresponding entites.
        let mut sets = Vec::default();
        for archetype in archetypes.archetypes() {
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
                        C::make_storage_set(archetype, archetypes),
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

impl<'a, C: ComponentFilter, T: TagFilter> Iterator for EntityComponentTagQuery<C, T> {
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
