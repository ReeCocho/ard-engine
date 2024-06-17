use std::{
    any::TypeId,
    num::NonZeroU8,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::{
    archetype::{ArchetypeId, Archetypes},
    component::pack::{ComponentPack, ComponentPackMover, EmptyComponentPack},
    entity::Entity,
    prelude::{Component, ComponentExt},
    tag::{
        pack::{EmptyTagPack, TagPack},
        Tag, TagCollectionId, TagExt, Tags,
    },
};

/// Container for entities belonging to a world.
pub struct Entities {
    /// List of entity descriptions.
    entities: Vec<EntityInfo>,
    entity_commands: EntityCommands,
    commands_receiver: Receiver<EntityCommand>,
    construction_data: Arc<EntityConstructionData>,
}

/// Data used when constructing new entities. This is passed to the `EntityCommands` object to
/// allow users to create entities in parallel without locking.
struct EntityConstructionData {
    /// Counter for the current maximum number of entities.
    max_entities: AtomicUsize,
    /// List of free entity handles.
    free_recv: Receiver<Entity>,
    free_send: Sender<Entity>,
}

/// Used to manipulated created entities.
#[derive(Clone)]
pub struct EntityCommands {
    sender: Sender<EntityCommand>,
    construction_data: Arc<EntityConstructionData>,
}

pub(crate) enum EntityCommand {
    Create {
        components: Box<ComponentPackMover>,
        tags: Option<Box<dyn TagPack>>,
        /// Entity handles for the newly created entities.
        entities: Vec<Entity>,
    },
    Destroy {
        entities: Vec<Entity>,
    },
    RemoveComponent {
        entity: Entity,
        id: TypeId,
    },
    SetComponents {
        entities: Vec<Entity>,
        components: Box<ComponentPackMover>,
    },
    SetTags {
        entities: Vec<Entity>,
        tags: Box<dyn TagPack>,
    },
    AddComponent {
        entity: Entity,
        component: Box<dyn ComponentExt>,
    },
    RemoveTag {
        entity: Entity,
        id: TypeId,
    },
    AddTag {
        entity: Entity,
        tag: Box<dyn TagExt>,
    },
}

/// Description of an entity within the world.
#[derive(Debug, Copy, Clone)]
pub(crate) struct EntityInfo {
    /// Current version of the entity.
    pub ver: NonZeroU8,
    /// ID of the archetype the entities components exist in.
    pub archetype: ArchetypeId,
    /// Index within each archetype buffer the entities components exists in.
    /// NOTE: You might wonder "why not a usize"? By making this a u32 we can get the size of
    /// this struct down to 16 from 24 which, imo, is a worthy compromise seeing as having more
    /// than 4 billion entities in a single archetype should (hopefully) never happen.
    pub index: u32,
    /// Index of the collection the entities tags exist in or `None` if the entity has no tags.
    pub collection: Option<TagCollectionId>,
}

impl Default for Entities {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        let (free_send, free_recv) = unbounded();

        let construction_data = Arc::new(EntityConstructionData {
            max_entities: AtomicUsize::new(0),
            free_recv,
            free_send,
        });

        let entity_commands = EntityCommands {
            sender,
            construction_data: construction_data.clone(),
        };

        Self {
            commands_receiver: receiver,
            entities: Vec::default(),
            entity_commands,
            construction_data,
        }
    }
}

impl Entities {
    #[inline]
    pub fn commands(&self) -> &EntityCommands {
        &self.entity_commands
    }

    /// Process entities pending creation an deletion.
    pub fn process(&mut self, archetypes: &mut Archetypes, tags: &mut Tags) {
        // Process commands
        for command in self.commands_receiver.clone().try_iter() {
            match command {
                EntityCommand::Create {
                    components,
                    tags: tag_pack,
                    entities,
                } => {
                    // No need to verify components and tags because they must have been verified
                    // when added to the `EntityCommands` object.

                    // Update entity info list if too short
                    let mut max_id = 0;
                    for entity in &entities {
                        if entity.id() > max_id {
                            max_id = entity.id();
                        }
                    }

                    if max_id as usize >= self.entities.len() {
                        self.entities.resize(
                            max_id as usize + 1,
                            EntityInfo {
                                ver: NonZeroU8::new(1).unwrap(),
                                archetype: ArchetypeId::default(),
                                index: 0,
                                collection: None,
                            },
                        );
                    }

                    // Move the components into their archetype
                    let (archetype, begin) = components(&entities, archetypes);

                    // Move tags into their storages if needed
                    let collection =
                        tag_pack.map(|mut tag_pack| tag_pack.move_into(&entities, tags));

                    // Update the created entities archetypes
                    for (i, entity) in entities.iter().enumerate() {
                        let info = &mut self.entities[entity.id() as usize];
                        info.archetype = archetype;
                        info.collection = collection;
                        info.index = (begin + i) as u32;
                    }
                }
                EntityCommand::Destroy { entities } => {
                    for mut entity in entities {
                        let ind = entity.id() as usize;

                        // Verify that the entity handles are valid
                        debug_assert!(ind < self.entities.len());

                        let info = &mut self.entities[ind];
                        debug_assert!(info.ver.get() == entity.ver() as u8);

                        // Update version counter
                        info.ver = unsafe {
                            NonZeroU8::new_unchecked(info.ver.get().wrapping_add(1).max(1))
                        };

                        // Remove tags if needed
                        if let Some(id) = &info.collection {
                            let collection = tags
                                .get_collection(*id)
                                .expect("Tag collection does not exist for entity");
                            for id in collection.type_key.iter() {
                                let storage = tags.get_storage_id(*id).unwrap();
                                tags.get_storage_by_id(storage).remove(&[entity]);
                            }
                        }

                        // Update entity handle with appropriate version
                        entity = Entity::new(entity.id(), info.ver);

                        // Remove components and swap the moved entity if needed
                        if let Some(moved_entity) =
                            archetypes.remove_entity(info.archetype, info.index as usize)
                        {
                            self.entities[moved_entity.id() as usize].index = info.index;
                        }

                        // Add entity to the free list
                        self.construction_data.free_send.send(entity).unwrap();
                    }
                }
                EntityCommand::RemoveComponent { entity, id } => {
                    let info = &mut self.entities[entity.id() as usize];
                    debug_assert!(info.ver.get() == entity.ver() as u8);

                    if let Some((new_archetype, new_index, moved_entity)) =
                        archetypes.remove_component(entity, info.archetype, info.index as usize, id)
                    {
                        let old_idx = info.index;
                        info.archetype = new_archetype;
                        info.index = new_index as u32;

                        if let Some(moved_entity) = moved_entity {
                            self.entities[moved_entity.id() as usize].index = old_idx;
                        }
                    }
                }
                EntityCommand::SetComponents {
                    entities,
                    components,
                } => {
                    // "Delete" all the entities so they have no components
                    for entity in &entities {
                        let ind = entity.id() as usize;

                        // Verify that the entity handles are valid
                        debug_assert!(ind < self.entities.len());

                        let info = &mut self.entities[ind];
                        debug_assert!(info.ver.get() == entity.ver() as u8);

                        // Remove components and swap the moved entity if needed
                        if let Some(moved_entity) =
                            archetypes.remove_entity(info.archetype, info.index as usize)
                        {
                            self.entities[moved_entity.id() as usize].index = info.index;
                        }
                    }

                    // Move the components into their archetype
                    let (archetype, begin) = components(&entities, archetypes);

                    // Update the entities archetypes
                    for (i, entity) in entities.iter().enumerate() {
                        let info = &mut self.entities[entity.id() as usize];
                        info.archetype = archetype;
                        info.index = (begin + i) as u32;
                    }
                }
                EntityCommand::SetTags {
                    entities,
                    tags: mut tag_pack,
                } => {
                    // Remove tags from each entities
                    for entity in &entities {
                        let ind = entity.id() as usize;
                        debug_assert!(ind < self.entities.len());

                        let info = &mut self.entities[ind];
                        debug_assert_eq!(info.ver.get(), entity.ver() as u8);

                        let collection = match info.collection {
                            Some(collection) => collection,
                            None => continue,
                        };

                        tags.remove_entity(*entity, collection);
                        info.collection = None;
                    }

                    let collection = tag_pack.move_into(&entities, tags);

                    for entity in &entities {
                        let ind = entity.id() as usize;
                        let info = &mut self.entities[ind];
                        info.collection = Some(collection);
                    }
                }
                EntityCommand::AddComponent { entity, component } => {
                    let info = &mut self.entities[entity.id() as usize];
                    debug_assert!(info.ver.get() == entity.ver() as u8);

                    if let Some((new_archetype, new_index, moved_entity)) = archetypes
                        .add_component(entity, info.archetype, info.index as usize, component)
                    {
                        let old_idx = info.index;
                        info.archetype = new_archetype;
                        info.index = new_index as u32;

                        if let Some(moved_entity) = moved_entity {
                            self.entities[moved_entity.id() as usize].index = old_idx;
                        }
                    }
                }
                EntityCommand::RemoveTag { entity, id } => {
                    let info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver() as u8);
                    if let Some(collection) = info.collection {
                        info.collection = tags.remove_tag(entity, id, collection);
                    }
                }
                EntityCommand::AddTag { entity, tag } => {
                    let info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver() as u8);
                    info.collection = Some(tags.add_tag(entity, info.collection, tag));
                }
            }
        }
    }

    #[inline]
    pub(crate) fn entities(&self) -> &[EntityInfo] {
        &self.entities
    }
}

impl EntityCommands {
    /// Requests that entities be destroyed.
    ///
    /// # Note
    /// See the note in `create_with_tags` about postponing creation. Same thing applies for
    /// deletion.
    ///
    /// # Panics
    /// This call will not panic if you provide invalid entity handles. However, the dispatcher
    /// will detect this during deferred destruction of entities and panic then.
    #[inline]
    pub fn destroy(&self, entities: &[Entity]) {
        if entities.is_empty() {
            return;
        }

        let _ = self.sender.send(EntityCommand::Destroy {
            entities: Vec::from(entities),
        });
    }

    /// Creates enough entities to fill in the slice. The created entities have no components.
    #[inline]
    pub fn create_empty(&self, entities: &mut [Entity]) {
        self.create(
            EmptyComponentPack {
                count: entities.len(),
            },
            entities,
        );
    }

    /// An alias for `EntityCommands::create_with_tags(components, (), entities)`.
    #[inline]
    pub fn create<C: ComponentPack + 'static>(&self, components: C, entities: &mut [Entity]) {
        let count = components.len();
        self.create_with_tags(components, EmptyTagPack { count }, entities);
    }

    /// Takes a component and tag pack of lengths 'm' and a mutable slice of length `n` of entity
    /// handles. The slice will be filled with the entities created by this command. If `n < m`,
    /// only the first `n` entity handles will be written. If `n > m`, the entity handles after the
    /// `n`th will remain untouched.
    ///
    /// # Note
    /// The entities created by this function are not actually initialized. This is to allow
    /// adding entities to a world while a query is operating on the same world. The consequence
    /// of this is that queries created during the same event dispatch as a call to this method
    /// will not detect the newly created entities.
    #[inline]
    pub fn create_with_tags<C: ComponentPack + 'static, T: TagPack + 'static>(
        &self,
        components: C,
        tags: T,
        entities: &mut [Entity],
    ) {
        debug_assert!(components.is_valid());
        debug_assert!(tags.is_valid());

        let new_entity_count = components.len();

        if tags.len() != 0 {
            debug_assert!(new_entity_count == tags.len());
        }

        // Construct new entity handles
        let mut new_entities = Vec::with_capacity(new_entity_count);

        // Grab handles off the free queue until we run out
        while let Ok(entity) = self.construction_data.free_recv.try_recv() {
            // Break early if we have created enough entities
            if new_entities.len() == new_entity_count {
                break;
            }

            // NOTE: We don't increment the version because it is updated during deletion
            new_entities.push(entity);
        }

        // If we still need new handles, use the free counter
        let needed = new_entity_count - new_entities.len();
        if needed > 0 {
            let base_id = self
                .construction_data
                .max_entities
                .fetch_add(needed, Ordering::Relaxed);

            for id in base_id..(base_id + needed) {
                new_entities.push(Entity::new(id as u32, unsafe {
                    NonZeroU8::new_unchecked(1)
                }))
            }
        }

        // Add handles to the output entities slice
        let elen = entities.len();
        entities[..(elen.min(new_entities.len()))]
            .copy_from_slice(&new_entities[..(elen.min(new_entities.len()))]);

        let _ = self.sender.send(EntityCommand::Create {
            components: Box::new(move |entities, archetypes| {
                components.move_into(entities, archetypes)
            }),
            tags: if tags.is_empty() {
                None
            } else {
                Some(Box::new(tags))
            },
            entities: new_entities,
        });
    }

    #[inline]
    pub fn set_components(&self, entities: &[Entity], components: impl ComponentPack + 'static) {
        debug_assert!(components.is_valid());
        debug_assert_eq!(entities.len(), components.len());

        let _ = self.sender.send(EntityCommand::SetComponents {
            entities: Vec::from(entities),
            components: Box::new(move |entities, archetypes| {
                components.move_into(entities, archetypes)
            }),
        });
    }

    #[inline]
    pub fn set_tags(&self, entities: &[Entity], tags: impl TagPack + 'static) {
        debug_assert!(tags.is_valid());
        debug_assert_eq!(entities.len(), tags.len());

        let _ = self.sender.send(EntityCommand::SetTags {
            entities: Vec::from(entities),
            tags: Box::new(tags),
        });
    }

    #[inline]
    pub fn add_component<C: ComponentExt + 'static>(&self, entity: Entity, component: C) {
        let _ = self.sender.send(EntityCommand::AddComponent {
            entity,
            component: Box::new(component),
        });
    }

    #[inline]
    pub fn remove_component<C: Component + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveComponent {
            entity,
            id: TypeId::of::<C>(),
        });
    }

    #[inline]
    pub fn add_tag(&self, entity: Entity, tag: impl Tag + 'static) {
        let _ = self.sender.send(EntityCommand::AddTag {
            entity,
            tag: Box::new(tag),
        });
    }

    #[inline]
    pub fn remove_tag<T: Tag + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveTag {
            entity,
            id: TypeId::of::<T>(),
        });
    }
}
