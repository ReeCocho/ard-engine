use std::{
    any::TypeId,
    num::NonZeroU32,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bitvec::macros::internal::funty::Fundamental;
use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::{
    archetype::{ArchetypeId, Archetypes},
    component::pack::ComponentPack,
    entity::Entity,
    prelude::{Component, ComponentExt},
    tag::{pack::TagPack, Tag, TagCollectionId, TagExt, Tags},
};

/// Container for entities belonging to a world.
pub struct Entities {
    /// List of entity descriptions.
    entities: Vec<EntityInfo>,
    /// List of entities to create when processing.
    to_create: Vec<EntityCreateInfo>,
    /// List of entities to destroy when processing.
    to_destroy: Vec<Entity>,
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
        components: Box<dyn ComponentPack>,
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
    pub ver: NonZeroU32,
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

/// A pack of data used to create a set of entities.
struct EntityCreateInfo {
    /// Component pack used to initialize the entities.  
    components: Box<dyn ComponentPack>,
    /// Optional tag pack used to initialize the entities.
    tags: Option<Box<dyn TagPack>>,
    /// Handles of the generated entities.
    entities: Vec<Entity>,
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
            sender: sender.clone(),
            construction_data: construction_data.clone(),
        };

        Self {
            commands_receiver: receiver,
            entities: Vec::default(),
            entity_commands,
            to_create: Vec::default(),
            to_destroy: Vec::default(),
            construction_data,
        }
    }
}

impl Entities {
    /// Determines the number of active entities.
    #[inline]
    pub fn count(&self) -> usize {
        self.entities.len() - self.construction_data.free_recv.len() - self.to_destroy.len()
    }

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
                    tags,
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
                                ver: unsafe { NonZeroU32::new_unchecked(1) },
                                archetype: ArchetypeId::default(),
                                index: 0,
                                collection: None,
                            },
                        );
                    }

                    // For defered initialization
                    self.to_create.push(EntityCreateInfo {
                        components,
                        tags,
                        entities,
                    });
                }
                EntityCommand::Destroy { entities } => {
                    for entity in &entities {
                        let ind = entity.id() as usize;

                        // Verify that the entity handles are valid
                        if ind >= self.entities.len()
                            || self.entities[ind].ver.get() != entity.ver().as_u32()
                        {
                            panic!("attempt to delete an invalid entity handle");
                        }

                        // Add to list
                        self.to_destroy.push(*entity);

                        // Update version counter
                        self.entities[ind].ver = unsafe {
                            NonZeroU32::new_unchecked(
                                self.entities[ind].ver.get().wrapping_add(1).max(1),
                            )
                        };
                    }
                }
                EntityCommand::RemoveComponent { entity, id } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    debug_assert!(info.ver.get() == entity.ver());

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
                EntityCommand::AddComponent { entity, component } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    debug_assert!(info.ver.get() == entity.ver());

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
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    if let Some(collection) = info.collection {
                        info.collection = tags.remove_tag(entity, id, collection);
                    }
                }
                EntityCommand::AddTag { entity, tag } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    info.collection = Some(tags.add_tag(entity, info.collection, tag));
                }
            }
        }

        // Process to create
        for mut pack in self.to_create.drain(..) {
            // Move the components into their archetype
            let (archetype, begin) = pack.components.move_into(&pack.entities, archetypes);

            // Move tags into their storages if needed
            let collection = if let Some(mut tag_pack) = pack.tags {
                Some(tag_pack.move_into(&pack.entities, tags))
            } else {
                None
            };

            // Update the created entities archetypes
            for (i, entity) in pack.entities.iter().enumerate() {
                let mut info = &mut self.entities[entity.id() as usize];
                info.archetype = archetype;
                info.collection = collection;
                info.index = (begin + i) as u32;
            }
        }

        // Process to destroy
        for entity in self.to_destroy.drain(..) {
            let info = &mut self.entities[entity.id() as usize];

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
        let _ = self.sender.send(EntityCommand::Destroy {
            entities: Vec::from(entities),
        });
    }

    /// An alias for `EntityCommands::create_with_tags(components, (), entities)`.
    #[inline]
    pub fn create<C: ComponentPack + 'static>(&self, components: C, entities: &mut [Entity]) {
        self.create_with_tags(components, (), entities);
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
                    NonZeroU32::new_unchecked(1)
                }))
            }
        }

        // Add handles to the output entities slice
        for i in 0..(entities.len().min(new_entities.len())) {
            entities[i] = new_entities[i];
        }

        let _ = self.sender.send(EntityCommand::Create {
            components: Box::new(components),
            tags: if tags.is_empty() {
                None
            } else {
                Some(Box::new(tags))
            },
            entities: new_entities,
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
